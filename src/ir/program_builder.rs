use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::builtins::builtin_functions::get_builtin_functions;
use crate::parser::ast::common::ResolveStatus;
use crate::parser::ast::expression::{BinaryOp, Expression, UnaryOp};
use crate::parser::ast::item::Item;
use crate::parser::ast::statement::Statement;
use crate::semantic::symbol_table::{SymbolInfoError, SymbolTableError};
use crate::{ir::typemap::typemap, parser::ast::compile_unit::CompileUnit};
use koopa::ir::builder::{BasicBlockBuilder, GlobalInstBuilder, LocalInstBuilder, ValueBuilder};
use koopa::ir::entities::ValueData;
use koopa::ir::{self, BasicBlock, Function, FunctionData, Program, Type, Value, ValueKind};
use thiserror::Error;

macro_rules! new_value {
    ($func_data:expr) => {
        new_instr!($func_data)
    };
}

macro_rules! new_instr {
    ($func_data:expr) => {
        $func_data.dfg_mut().new_value()
    };
}

macro_rules! add_instr {
    ($func_data:expr, $bb:expr, $instr:expr) => {
        $func_data
            .layout_mut()
            .bb_mut($bb)
            .insts_mut()
            .push_key_back($instr)
            .unwrap();
    };
}

macro_rules! new_bb {
    ($func_data:expr, $name:expr) => {
        $func_data
            .dfg_mut()
            .new_bb()
            .basic_block(Some($name.to_string()))
    };
}

macro_rules! add_bb {
    ($func_data:expr, $bb:expr) => {
        $func_data
            .layout_mut()
            .bbs_mut()
            .push_key_back($bb)
            .unwrap();
    };
}

/// 存放生成ir时所需的上下文
#[derive(Debug, Clone)]
struct Context {
    /// 生成当前内容时所在的基本块
    in_block: BasicBlock,

    /// 当前所在的while的开头，即该while的guard块
    while_begin: Option<BasicBlock>,

    /// 当前所在的while结束后的第一个基本块
    while_end: Option<BasicBlock>,
}

#[derive(Debug, Clone)]
struct NameGenerator {
    used_names: HashMap<String, usize>,
}

impl NameGenerator {
    fn new() -> Self {
        Self {
            used_names: HashMap::new(),
        }
    }

    fn generate(&mut self, name: &str) -> String {
        self.used_names
            .entry(name.to_owned())
            .and_modify(|count| *count = *count + 1)
            .or_insert(0);
        let count = self.used_names.get(name).unwrap();
        format!("{}_{}", name, count)
    }
}

static NAME_GENERATOR: OnceLock<std::sync::Mutex<NameGenerator>> = OnceLock::new();

fn get_name_generator() -> &'static std::sync::Mutex<NameGenerator> {
    NAME_GENERATOR.get_or_init(|| std::sync::Mutex::new(NameGenerator::new()))
}

macro_rules! generate_name {
    ($name:expr) => {
        get_name_generator().lock().unwrap().generate($name)
    };
}

// 获取基本块的最后一条指令
fn last_instr(func_data: &mut FunctionData, bb: BasicBlock) -> Option<&ValueData> {
    let last_instr = func_data
        .layout_mut()
        .bb_mut(bb)
        .insts()
        .back_key()
        .cloned();
    match last_instr {
        Some(v) => Some(func_data.dfg().value(v)),
        None => None,
    }
}

#[derive(Debug, Error)]
pub enum ProgramBuilderError {
    #[error("symbol table error: {0}")]
    SymbolTableError(#[from] SymbolTableError),

    #[error("symbol information error: {0}")]
    SymbolInfoError(#[from] SymbolInfoError),

    #[error("symbol-value map error: {0}")]
    SymbolValueMapError(#[from] SymbolValueMapError),
}

pub struct ProgramBuilder {
    ast: CompileUnit,
    symbol_value_map: SymbolValueMap,
}

impl ProgramBuilder {
    pub fn new(ast: CompileUnit) -> Self {
        Self {
            ast,
            symbol_value_map: SymbolValueMap::new(),
        }
    }

    pub fn build_compunit(&mut self) -> Result<Program, ProgramBuilderError> {
        let mut prog = Program::new();

        // build编译器内置的item
        let builtin_funcs_guard = get_builtin_functions().lock().unwrap();
        for (id, syminfo) in builtin_funcs_guard.iter() {
            let func_koopa_ty = typemap(&syminfo.ty);
            let (params_koopa_ty, ret_koopa_ty) = match func_koopa_ty.kind() {
                ir::TypeKind::Function(p, r) => (p, r),
                _ => unreachable!(),
            };
            let func = prog.new_func(FunctionData::new_decl(
                format!("@{}", id),
                params_koopa_ty.clone(),
                ret_koopa_ty.clone(),
            ));

            // 把函数加入symbol_value_map
            self.symbol_value_map.add_function(id.clone(), func);
        }

        // build用户实现的item
        let items = &self.ast.items.clone();
        for item in items {
            self.build_item(&mut prog, item)?;
        }
        Ok(prog)
    }

    fn build_item(&mut self, prog: &mut Program, item: &Item) -> Result<(), ProgramBuilderError> {
        match item {
            Item::FuncDef(f) => {
                // 创建函数框架
                let func = prog.new_func(FunctionData::new(
                    format!("@{}", f.ident.name),
                    f.fparams.iter().map(|(ty, _)| typemap(ty)).collect(),
                    typemap(&f.return_type),
                ));
                let func_data = prog.func_mut(func);
                // dbg!(func_data.params());
                let entry_bb = new_bb!(func_data, generate_name!("%entry"));
                add_bb!(func_data, entry_bb);

                // 把函数加入symbol_value_map
                self.symbol_value_map
                    .add_function(f.ident.name.clone(), func);

                // 进入子作用域
                let new_scope = self.symbol_value_map.enter_scope();
                self.symbol_value_map = new_scope;

                // 在函数体中为参数分配空间，并将ident绑定到分配的空间
                let (params_types, params_names) =
                    f.fparams
                        .iter()
                        .fold((Vec::new(), Vec::new()), |mut acc, (ty, id)| {
                            acc.0.push(ty);
                            acc.1.push(id);
                            (acc.0, acc.1)
                        });
                let params_values = func_data.params().to_vec();
                for i in 0..params_names.len() {
                    let alloc = new_instr!(func_data).alloc(typemap(params_types[i]));
                    add_instr!(func_data, entry_bb, alloc);
                    let store = new_instr!(func_data).store(params_values[i], alloc);
                    add_instr!(func_data, entry_bb, store);
                    self.symbol_value_map
                        .add_value(params_names[i].name.clone(), alloc);
                }

                // 生成函数中的语句
                let mut bb = entry_bb;
                for stmt in &f.body {
                    bb = self
                        .build_stmt(
                            stmt,
                            func_data,
                            &Context {
                                in_block: bb,
                                while_begin: None,
                                while_end: None,
                            },
                        )?
                        .in_block;

                    // 如果生成的最后一条语句是return语句，则判断后面的语句为不可达，直接跳过
                    let last_instr = last_instr(func_data, bb);
                    if last_instr.is_some()
                        && matches!(last_instr.unwrap().kind(), ValueKind::Return(_))
                    {
                        break;
                    }
                }

                // 如果最后一条语句不存在（即函数为空），则添加一条返回语句
                let last_instr = last_instr(func_data, bb);
                if last_instr.is_none() {
                    let ret = new_instr!(func_data).ret(None);
                    add_instr!(func_data, bb, ret);
                } else {
                    // 如果最后一条语句存在且不是return，则添加一条返回语句
                    if !matches!(last_instr.unwrap().kind(), ValueKind::Return(_)) {
                        let ret = new_instr!(func_data).ret(None);
                        add_instr!(func_data, bb, ret);
                    }
                }

                // 返回父作用域
                let old_scope = self.symbol_value_map.exit_scope()?;
                self.symbol_value_map = old_scope;

                Ok(())
            }
            Item::GlobalVar(stmt) => match stmt {
                Statement::VarDecl(v) => {
                    for (ty, id, expr_opt) in v {
                        let alloc = match expr_opt {
                            Some(expr) => {
                                let init =
                                    prog.new_value().integer(self.calc_global_expr_value(expr)?);
                                let alloc = prog.new_value().global_alloc(init);
                                prog.set_value_name(alloc, Some("%".to_string() + &id.name)); // 设置name，以便生成汇编时引用
                                alloc
                            }
                            None => {
                                let init = prog.new_value().zero_init(typemap(ty));
                                let alloc = prog.new_value().global_alloc(init);
                                prog.set_value_name(alloc, Some("%".to_string() + &id.name)); // 设置name，以便生成汇编时引用
                                alloc
                            }
                        };
                        self.symbol_value_map.add_value(id.name.clone(), alloc);
                    }
                    Ok(())
                }
                Statement::ConstDecl(_) => Ok(()), // 没必要生成ir，用到时直接调用syminfo中的const_val_of()来获取其值即可
                _ => unreachable!(),
            },
        }
    }

    // 返回值是该语句后的代码应该在其中生成的基本块
    fn build_stmt(
        &mut self,
        stmt: &Statement,
        func_data: &mut FunctionData,
        ctx: &Context,
    ) -> Result<Context, ProgramBuilderError> {
        match stmt {
            Statement::Return(expr) => {
                let (ret_val, ret_ctx) = self.build_expr(expr, func_data, &ctx)?;
                let ret_stmt = func_data.dfg_mut().new_value().ret(Some(ret_val));
                add_instr!(func_data, ret_ctx.in_block, ret_stmt);
                Ok(ret_ctx)
            }
            Statement::ConstDecl(_) => Ok(ctx.clone()),
            Statement::VarDecl(v) => {
                let mut new_ctx_outer = ctx.clone(); // 初始设为当前ctx
                for (_, id, expr_opt) in v {
                    let alloc = new_value!(func_data).alloc(Type::get_i32());
                    add_instr!(func_data, ctx.in_block, alloc);
                    self.symbol_value_map.add_value(id.name.clone(), alloc);
                    if let Some(expr) = expr_opt {
                        let (val, new_ctx) = self.build_expr(expr, func_data, ctx)?;
                        let store = new_value!(func_data).store(val, alloc);
                        add_instr!(func_data, new_ctx.in_block, store);
                        new_ctx_outer = new_ctx; // 有新的ctx再设为新的ctx
                    }
                }
                Ok(new_ctx_outer.clone())
            }
            Statement::Assign(lval, expr) => {
                let lval = self.symbol_value_map.find_value(&lval.name);
                let (value, new_ctx) = self.build_expr(expr, func_data, ctx)?;
                let instr = new_instr!(func_data).store(value, lval);
                add_instr!(func_data, new_ctx.in_block, instr);
                Ok(new_ctx.clone())
            }
            Statement::Expression(expr_opt) => match expr_opt {
                Some(expr) => self
                    .build_expr(expr, func_data, ctx)
                    .map(|(_, new_ctx)| new_ctx.clone()),
                None => Ok(ctx.clone()),
            },
            Statement::Block(b) => {
                // 进入子作用域
                let new_scope = self.symbol_value_map.enter_scope();
                self.symbol_value_map = new_scope;

                // 生成块中的语句
                let mut next_ctx = ctx.clone();
                for stmt in b {
                    next_ctx = self.build_stmt(*&stmt, func_data, &next_ctx)?;
                }

                // 返回父作用域
                let parent_scope = self.symbol_value_map.exit_scope()?;
                self.symbol_value_map = parent_scope;

                Ok(next_ctx) // XXX: 返回最后一个语句生成后返回的ctx
            }
            Statement::If(guard, then_br, else_br) => {
                // 生成并添加guard块
                let guard_bb = new_bb!(func_data, generate_name!("%guard"));
                add_bb!(func_data, guard_bb);
                // 在当前位置添加跳转到guard块的br指令
                let jump_to_guard = new_instr!(func_data).jump(guard_bb);
                add_instr!(func_data, ctx.in_block, jump_to_guard);

                // 生成并添加merge块
                let merge_bb = new_bb!(func_data, generate_name!("%merge"));
                add_bb!(func_data, merge_bb);
                // 负责跳转到merge块的指令
                let jump_to_merge = new_instr!(func_data).jump(merge_bb);

                // 生成then块
                let then_bb = new_bb!(func_data, generate_name!("%then"));
                add_bb!(func_data, then_bb);
                // 生成完then块后的ctx
                let then_ctx = self.build_stmt(
                    then_br.as_ref(),
                    func_data,
                    &Context {
                        in_block: then_bb,
                        ..ctx.clone()
                    },
                )?;
                // 若完成指令生成后，最后一个指令不存在/不为跳转指令，则将jump_to_merge指令加入
                let then_bb_last_instr = last_instr(func_data, then_ctx.in_block);
                if then_bb_last_instr.is_none() || !Self::is_jump(then_bb_last_instr.unwrap()) {
                    add_instr!(func_data, then_ctx.in_block, jump_to_merge);
                }

                // 生成else块
                let else_bb = match else_br {
                    Some(else_br) => {
                        // 在else块里生成指令
                        let else_bb = new_bb!(func_data, generate_name!("%else"));
                        add_bb!(func_data, else_bb);
                        let else_ctx = self.build_stmt(
                            else_br.as_ref(),
                            func_data,
                            &Context {
                                in_block: else_bb,
                                ..ctx.clone()
                            },
                        )?;

                        // 若完成指令生成后，最后一个指令不存在/不为跳转指令，则将jump_to_merge指令加入
                        let else_bb_last_instr = last_instr(func_data, else_ctx.in_block);
                        if else_bb_last_instr.is_none()
                            || !Self::is_jump(else_bb_last_instr.unwrap())
                        {
                            add_instr!(func_data, else_ctx.in_block, jump_to_merge);
                        }

                        Some(else_bb)
                    }
                    None => None, // 用一个幽灵块来规避类型检查器的检查
                };

                // 生成guard及跳转代码
                let (guard_ir, guard_ctx) = self.build_expr(
                    guard,
                    func_data,
                    &Context {
                        in_block: guard_bb,
                        ..ctx.clone()
                    },
                )?;
                let branch_ir = match else_bb {
                    Some(else_bb) => new_instr!(func_data).branch(guard_ir, then_bb, else_bb),
                    None => new_instr!(func_data).branch(guard_ir, then_bb, merge_bb),
                };
                add_instr!(func_data, guard_ctx.in_block, branch_ir);

                Ok(Context {
                    in_block: merge_bb,
                    while_begin: None,
                    while_end: None,
                })
            }
            Statement::While(expr, stmt) => {
                let guard_bb = new_bb!(func_data, generate_name!("%while_guard"));
                let body_bb = new_bb!(func_data, generate_name!("%while_body"));
                let end_bb = new_bb!(func_data, generate_name!("%while_end"));

                // 保存外层while的信息
                let (outer_while_begin, outer_while_end) = (ctx.while_begin, ctx.while_end);

                // 将与while有关的块加入
                add_bb!(func_data, guard_bb);
                add_bb!(func_data, body_bb);
                add_bb!(func_data, end_bb);

                // 在当前块生成跳转到guard块的指令
                let jump_to_guard = new_instr!(func_data).jump(guard_bb);
                add_instr!(func_data, ctx.in_block, jump_to_guard);

                // 生成guard块中的代码
                let (guard, guard_ctx) = self.build_expr(
                    expr,
                    func_data,
                    &Context {
                        in_block: guard_bb,
                        ..ctx.clone()
                    },
                )?;
                let branch = new_instr!(func_data).branch(guard, body_bb, end_bb);
                add_instr!(func_data, guard_ctx.in_block, branch);

                // 生成body块中的代码
                let ctx_after_gen_body = self.build_stmt(
                    stmt,
                    func_data,
                    &Context {
                        in_block: body_bb,
                        while_begin: Some(guard_bb),
                        while_end: Some(end_bb),
                    },
                )?;
                // 如果body块（或其返回的ctx中的in_block）的最后一行为空（如空块语句{}）/不是跳转指令（如ret），
                // 则生成jump %while_guard指令
                let body_bb_last_instr = last_instr(func_data, ctx_after_gen_body.in_block);
                if body_bb_last_instr.is_none() || !Self::is_jump(body_bb_last_instr.unwrap()) {
                    add_instr!(func_data, ctx_after_gen_body.in_block, jump_to_guard);
                }

                Ok(Context {
                    in_block: end_bb,
                    while_begin: outer_while_begin,
                    while_end: outer_while_end,
                })
            }
            Statement::Break => {
                let jump_to_end = new_instr!(func_data).jump(ctx.while_end.unwrap());
                add_instr!(func_data, ctx.in_block, jump_to_end);

                // 为break的后续指令创建新块（虽然unreachable，但还是要创建）
                let after_break_bb = new_bb!(func_data, generate_name!("%after_break"));
                add_bb!(func_data, after_break_bb);

                Ok(Context {
                    in_block: after_break_bb,
                    ..ctx.clone()
                })
            }
            Statement::Continue => {
                let jump_to_begin = new_instr!(func_data).jump(ctx.while_begin.unwrap());
                add_instr!(func_data, ctx.in_block, jump_to_begin);

                // 为continue后续指令创建新块（虽然unreachable，但还是要创建）
                let after_continue_bb = new_bb!(func_data, generate_name!("%after_continue"));
                add_bb!(func_data, after_continue_bb);

                Ok(Context {
                    in_block: after_continue_bb,
                    ..ctx.clone()
                })
            }
        }
    }

    /// 判断指令是否为跳转指令
    fn is_jump(instr: &ValueData) -> bool {
        match instr.kind() {
            ValueKind::Jump(_) | ValueKind::Return(_) | ValueKind::Branch(_) => true,
            _ => false,
        }
    }

    /// 处理短路求值
    fn handle_short_circuit_eval(
        &self,
        expr: &Expression,
        func_data: &mut FunctionData,
        ctx: &Context,
    ) -> Result<(Value, Context), ProgramBuilderError> {
        match expr {
            Expression::Binary(lhs, binop, rhs) => {
                // 预备好基本块
                let short_circuit_block =
                    new_bb!(func_data, generate_name!("@short_circuit_block"));
                let eval_rhs_block = new_bb!(func_data, generate_name!("@eval_rhs_block"));
                let short_circuit_merge =
                    new_bb!(func_data, generate_name!("@short_circuit_merge"));
                add_bb!(func_data, short_circuit_block);
                add_bb!(func_data, eval_rhs_block);
                add_bb!(func_data, short_circuit_merge);

                // 用于存储最终结果的存储单元
                let final_result = new_instr!(func_data).alloc(Type::get_i32());
                add_instr!(func_data, ctx.in_block, final_result);

                // 生成lhs对应的代码
                let (lhs_val, lhs_ctx) = self.build_expr(lhs, func_data, ctx)?;

                // 入口块：根据lhs的值判断是否需要跳转
                let br = match binop {
                    BinaryOp::LogicalOr => {
                        new_instr!(func_data).branch(lhs_val, short_circuit_block, eval_rhs_block)
                    }

                    BinaryOp::LogicalAnd => {
                        new_instr!(func_data).branch(lhs_val, eval_rhs_block, short_circuit_block)
                    }
                    _ => unreachable!(),
                };
                add_instr!(func_data, lhs_ctx.in_block, br);

                // 短路结果块：直接得出结果并存到final_result中
                match binop {
                    BinaryOp::LogicalOr => {
                        let result = new_value!(func_data).integer(1);
                        let store = new_instr!(func_data).store(result, final_result);
                        add_instr!(func_data, short_circuit_block, store);
                    }
                    BinaryOp::LogicalAnd => {
                        let result = new_value!(func_data).integer(0);
                        let store = new_instr!(func_data).store(result, final_result);
                        add_instr!(func_data, short_circuit_block, store);
                    }
                    _ => unreachable!(),
                };
                let jump_to_merge = new_instr!(func_data).jump(short_circuit_merge);
                add_instr!(func_data, short_circuit_block, jump_to_merge); // 跳到merge块

                // 计算rhs块：计算rhs的结果并存到final_result中
                let (rhs_val, rhs_ctx) = self.build_expr(
                    rhs,
                    func_data,
                    &Context {
                        in_block: eval_rhs_block,
                        ..lhs_ctx.clone()
                    },
                )?;
                let store = new_instr!(func_data).store(rhs_val, final_result);
                add_instr!(func_data, rhs_ctx.in_block, store);
                add_instr!(func_data, rhs_ctx.in_block, jump_to_merge); // 跳到merge块

                // dbg!(
                //     func_data.dfg().value(lhs_val).ty(),
                //     func_data.dfg().value(rhs_val).ty(),
                //     func_data.dfg().value(final_result).ty()
                // );

                // 合并块：后续代码在此处生成
                Ok((
                    final_result,
                    Context {
                        in_block: short_circuit_merge,
                        ..rhs_ctx.clone()
                    },
                ))
            }

            // 非binary表达式，也就不可能是逻辑与/或表达式
            expr => self.build_expr(expr, func_data, ctx),
        }
    }

    fn calc_global_expr_value(&self, expr: &Expression) -> Result<i32, ProgramBuilderError> {
        match expr {
            Expression::IntLit(i) => Ok(*i),
            Expression::Ident(id) => {
                let syminfo = match &*id.resolve_status.borrow() {
                    ResolveStatus::Resolved(syminfo) => syminfo.clone(),
                    ResolveStatus::Unresolved => {
                        unreachable!()
                    }
                };
                if syminfo.is_const_val() {
                    return Ok(syminfo.const_val_of().unwrap());
                } else {
                    unreachable!(); // FIXME: 在语义检查阶段就要检查全局变量表达式的组成部分是否都为常量表达式
                }
            }
            Expression::Unary(op, expr) => match op {
                UnaryOp::Plus => self.calc_global_expr_value(expr),
                UnaryOp::Minus => self.calc_global_expr_value(expr).map(|result| -result),
                UnaryOp::LogicalNot => {
                    let result = self.calc_global_expr_value(expr)?;
                    if result == 0 {
                        return Ok(1);
                    } else {
                        return Ok(0);
                    }
                }
            },
            Expression::Binary(lhs, op, rhs) => {
                let lhs_result = self.calc_global_expr_value(lhs)?;
                let rhs_result = self.calc_global_expr_value(rhs)?;
                let i32_to_bool = |i: i32| i != 0;
                let result = match op {
                    // 算数运算符
                    BinaryOp::Add => lhs_result + rhs_result,
                    BinaryOp::Sub => lhs_result - rhs_result,
                    BinaryOp::Mul => lhs_result * rhs_result,
                    BinaryOp::Div => lhs_result / rhs_result,
                    BinaryOp::Rem => lhs_result % rhs_result,

                    // 比较运算符
                    BinaryOp::Less => (lhs_result < rhs_result) as i32,
                    BinaryOp::Le => (lhs_result <= rhs_result) as i32,
                    BinaryOp::Eq => (lhs_result == rhs_result) as i32,
                    BinaryOp::Ge => (lhs_result >= rhs_result) as i32,
                    BinaryOp::Greater => (lhs_result > rhs_result) as i32,
                    BinaryOp::NotEq => (lhs_result != rhs_result) as i32,

                    // 逻辑运算符
                    BinaryOp::LogicalAnd => {
                        (i32_to_bool(lhs_result) && i32_to_bool(rhs_result)) as i32
                    }
                    BinaryOp::LogicalOr => {
                        (i32_to_bool(lhs_result) || i32_to_bool(rhs_result)) as i32
                    }
                };
                Ok(result)
            }
            _ => unreachable!(),
        }
    }

    fn build_expr(
        &self,
        expr: &Expression,
        func_data: &mut FunctionData,
        ctx: &Context,
    ) -> Result<(Value, Context), ProgramBuilderError> {
        match expr {
            Expression::IntLit(i) => Ok((func_data.dfg_mut().new_value().integer(*i), ctx.clone())),
            Expression::Ident(id) => {
                let syminfo = match &*id.resolve_status.borrow() {
                    ResolveStatus::Resolved(syminfo) => syminfo.clone(),
                    ResolveStatus::Unresolved => {
                        dbg!(id);
                        unreachable!()
                    }
                };
                match syminfo.is_const_val() {
                    true => {
                        let val = syminfo.const_val_of()?;
                        Ok((new_value!(func_data).integer(val), ctx.clone()))
                    }
                    false => {
                        let value = self.symbol_value_map.find_value(&id.name);
                        let instr = new_instr!(func_data).load(value);
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok((instr, ctx.clone()))
                    }
                }
            }
            Expression::Unary(op, expr) => {
                let (value, ctx) = self.build_expr(expr, func_data, ctx)?;
                let zero = func_data.dfg_mut().new_value().integer(0);
                match op {
                    UnaryOp::Plus => Ok((value, ctx.clone())),
                    UnaryOp::Minus => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Sub,
                            zero,
                            value,
                        );
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok((instr, ctx.clone()))
                    }
                    UnaryOp::LogicalNot => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Eq,
                            zero,
                            value,
                        );
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok((instr, ctx.clone()))
                    }
                }
            }
            Expression::Binary(e1, op, e2) => {
                // 首先判断是否需要短路求值，若是则提前操作，这样就不用执行下面求lhs和rhs的语句
                // 从而多生成一遍lhs和rhs的代码了
                if matches!(op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr) {
                    let (result, result_ctx) =
                        self.handle_short_circuit_eval(expr, func_data, &ctx)?;
                    let load = new_instr!(func_data).load(result);
                    add_instr!(func_data, result_ctx.in_block, load);
                    return Ok((load, result_ctx.clone()));
                }

                // 不是短路求值，正常处理
                let (v1, ctx1) = self.build_expr(e1, func_data, ctx)?;
                let (v2, ctx2) = self.build_expr(e2, func_data, &ctx1)?;
                let zero = new_instr!(func_data).integer(0);
                match op {
                    BinaryOp::Add => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Add,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::Sub => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Sub,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::Mul => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Mul,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::Div => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Div,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::Rem => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Mod,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::Less => {
                        let instr =
                            func_data
                                .dfg_mut()
                                .new_value()
                                .binary(koopa::ir::BinaryOp::Lt, v1, v2);
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::Le => {
                        let instr =
                            func_data
                                .dfg_mut()
                                .new_value()
                                .binary(koopa::ir::BinaryOp::Le, v1, v2);
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::Eq => {
                        let instr =
                            func_data
                                .dfg_mut()
                                .new_value()
                                .binary(koopa::ir::BinaryOp::Eq, v1, v2);
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::Ge => {
                        let instr = new_instr!(func_data).binary(koopa::ir::BinaryOp::Ge, v1, v2);
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::Greater => {
                        let instr = new_instr!(func_data).binary(koopa::ir::BinaryOp::Gt, v1, v2);
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::NotEq => {
                        let instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v1, v2);
                        add_instr!(func_data, ctx2.in_block, instr);
                        Ok((instr, ctx2.clone()))
                    }
                    BinaryOp::LogicalAnd | BinaryOp::LogicalOr => {
                        unreachable!(); // 已在本函数开头处理
                    }
                }
            }
            Expression::Call(identifier, expressions) => {
                // dbg!(&identifier.name);
                // dbg!(&self.symbol_value_map);
                let callee = self.symbol_value_map.find_function(&identifier.name);
                let mut args = vec![];
                let mut current_ctx = ctx.clone();
                for expr in expressions {
                    let (value, ctx) = self.build_expr(expr, func_data, &current_ctx)?;
                    args.push(value);
                    current_ctx = ctx;
                }
                let call = new_instr!(func_data).call(callee, args);
                add_instr!(func_data, current_ctx.in_block, call);

                Ok((call, current_ctx))
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum SymbolValueMapError {
    #[error("no parent scope")]
    NoParentScope,
}

#[derive(Debug, Clone)]
struct SymbolValueMap {
    parent: Option<Rc<RefCell<Self>>>,
    map: HashMap<String, Value>,
    func_map: HashMap<String, Function>,
}

impl SymbolValueMap {
    fn new() -> Self {
        Self {
            parent: None,
            map: HashMap::new(),
            func_map: HashMap::new(),
        }
    }

    /// 往当前层级添加ident-value映射
    fn add_value(&mut self, id: String, value: Value) {
        self.map.insert(id, value);
    }

    /// 从当前层级逐级往上查找ident
    fn find_value(&self, id: &str) -> Value {
        match self.map.get(id) {
            Some(val) => *val,
            None => match &self.parent {
                Some(parent) => (*parent).borrow().find_value(id),
                None => unreachable!(),
            },
        }
    }

    /// 往当前层级添加ident-function映射
    fn add_function(&mut self, id: String, func: Function) {
        self.func_map.insert(id, func);
    }

    /// 从当前层级逐级往上查找ident
    fn find_function(&self, id: &str) -> Function {
        match self.func_map.get(id) {
            Some(func) => *func,
            None => match &self.parent {
                Some(parent) => (*parent).borrow().find_function(id),
                None => unreachable!(),
            },
        }
    }

    fn enter_scope(&self) -> Self {
        let mut new_scope = Self::new();
        new_scope.parent = Some(Rc::new(RefCell::new(self.clone())));
        new_scope
    }

    fn exit_scope(&self) -> Result<Self, SymbolValueMapError> {
        match self.parent.as_ref() {
            Some(parent) => Ok((*parent).borrow().clone()),
            None => Err(SymbolValueMapError::NoParentScope),
        }
    }
}
