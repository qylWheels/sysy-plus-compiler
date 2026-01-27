use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::OnceLock;

use crate::parser::ast::common::ResolveStatus;
use crate::parser::ast::expression::{BinaryOp, Expression, UnaryOp};
use crate::parser::ast::item::Item;
use crate::parser::ast::statement::Statement;
use crate::semantic::symbol_table::{SymbolInfoError, SymbolTableError};
use crate::{ir::typemap::typemap, parser::ast::compile_unit::CompileUnit};
use koopa::ir::builder::{BasicBlockBuilder, LocalInstBuilder, ValueBuilder};
use koopa::ir::entities::ValueData;
use koopa::ir::{BasicBlock, FunctionData, Program, Type, Value, ValueKind};
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
    NAME_GENERATOR.get_or_init(|| {
        std::sync::Mutex::new(NameGenerator {
            used_names: HashMap::new(),
        })
    })
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
        let items = &self.ast.items.clone();
        for item in items {
            self.build_item(&mut prog, item)?;
        }
        Ok(prog)
    }

    fn build_item(&mut self, prog: &mut Program, item: &Item) -> Result<(), ProgramBuilderError> {
        match item {
            Item::FuncDef(f) => {
                // 进入子作用域
                let new_scope = self.symbol_value_map.enter_scope();
                self.symbol_value_map = new_scope;

                // 创建函数框架
                let func = prog.new_func(FunctionData::new(
                    format!("@{}", f.ident.name),
                    vec![],
                    typemap(&f.return_type),
                ));
                let func_data = prog.func_mut(func);
                let entry_bb = new_bb!(
                    func_data,
                    generate_name!("%entry")
                );
                add_bb!(func_data, entry_bb);

                // 生成函数中的语句
                let mut bb = entry_bb;
                for stmt in &f.body {
                    bb = self
                        .build_stmt(stmt, func_data, &Context { in_block: bb })?
                        .in_block;

                    // 如果生成的最后一条语句是return语句，则判断后面的语句为不可达，直接跳过
                    let last_instr = last_instr(func_data, bb);
                    if last_instr.is_some()
                        && matches!(last_instr.unwrap().kind(), ValueKind::Return(_))
                    {
                        break;
                    }
                }

                // 返回父作用域
                let old_scope = self.symbol_value_map.exit_scope()?;
                self.symbol_value_map = old_scope;

                Ok(())
            }
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
                let ret_val = self.build_expr(expr, func_data, &ctx)?;
                let ret_stmt = func_data.dfg_mut().new_value().ret(Some(ret_val));
                add_instr!(func_data, ctx.in_block, ret_stmt);
                Ok(ctx.clone())
            }
            Statement::ConstDecl(_) => Ok(ctx.clone()),
            Statement::VarDecl(v) => {
                for (_, id, expr_opt) in v {
                    let alloc = new_value!(func_data).alloc(Type::get_i32());
                    add_instr!(func_data, ctx.in_block, alloc);
                    self.symbol_value_map.add_symbol(id.name.clone(), alloc);
                    if let Some(expr) = expr_opt {
                        let val = self.build_expr(expr, func_data, ctx)?;
                        let store = new_value!(func_data).store(val, alloc);
                        add_instr!(func_data, ctx.in_block, store);
                    }
                }
                Ok(ctx.clone())
            }
            Statement::Assign(lval, expr) => {
                let lval = self.symbol_value_map.find_symbol(&lval.name);
                let value = self.build_expr(expr, func_data, ctx)?;
                let instr = new_instr!(func_data).store(value, lval);
                add_instr!(func_data, ctx.in_block, instr);
                Ok(ctx.clone())
            }
            Statement::Expression(expr_opt) => match expr_opt {
                Some(expr) => self.build_expr(expr, func_data, ctx).map(|_| ctx.clone()),
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
                let then_ctx =
                    self.build_stmt(then_br.as_ref(), func_data, &Context { in_block: then_bb })?;
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
                            &Context { in_block: else_bb },
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
                // TODO: 实现短路求值
                // self.handle_short_circuit_evaluation(guard, func_data, bb, then_bb, else_bb)?;
                let guard_ir =
                    self.build_expr(guard, func_data, &Context { in_block: guard_bb })?;
                let branch_ir = match else_bb {
                    Some(else_bb) => new_instr!(func_data).branch(guard_ir, then_bb, else_bb),
                    None => new_instr!(func_data).branch(guard_ir, then_bb, merge_bb),
                };
                add_instr!(func_data, guard_bb, branch_ir);

                Ok(Context { in_block: merge_bb })
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

    // fn handle_short_circuit_evaluation(
    //     &mut self,
    //     expr: &Expression,
    //     func_data: &mut FunctionData,
    //     entry: BasicBlock,
    //     then_br: BasicBlock,
    //     else_br: BasicBlock,
    //     result: Value,
    // ) -> Result<Value, ProgramBuilderError> {
    // match expr {
    //     Expression::Binary(lhs, binop, rhs) => {
    //         // 生成lhs和rhs的代码
    //         let lhs_result=self.handle_short_circuit_evaluation(lhs, func_data, entry, then_br, else_br,result)?;
    //         let rhs_result=self.handle_short_circuit_evaluation(rhs, func_data, entry, then_br, else_br,result)?;

    //         // 生成访存指令
    //         let load=new_instr!(func_data).load(lhs_result);
    //         add_instr!(func_data, entry, load);

    //         match binop {
    //             BinaryOp::LogicalOr => {
    //                 let branch=new_instr!(func_data).branch(load, then_br, false_bb)
    //             }
    //         }
    //     }
    // }

    //     Ok(result)
    // }

    fn build_expr(
        &self,
        expr: &Expression,
        func_data: &mut FunctionData,
        ctx: &Context,
    ) -> Result<Value, ProgramBuilderError> {
        match expr {
            Expression::IntLit(i) => Ok(func_data.dfg_mut().new_value().integer(*i)),
            Expression::Ident(id) => {
                let syminfo = match &*id.resolve_status.borrow() {
                    ResolveStatus::Resolved(syminfo) => syminfo.clone(),
                    ResolveStatus::Unresolved => unreachable!(),
                };
                match syminfo.is_const_val() {
                    true => {
                        let val = syminfo.const_val_of()?;
                        Ok(new_value!(func_data).integer(val))
                    }
                    false => {
                        let value = self.symbol_value_map.find_symbol(&id.name);
                        let instr = new_instr!(func_data).load(value);
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                }
            }
            Expression::Unary(op, expr) => {
                let value = self.build_expr(expr, func_data, ctx)?;
                let zero = func_data.dfg_mut().new_value().integer(0);
                match op {
                    UnaryOp::Plus => Ok(value),
                    UnaryOp::Minus => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Sub,
                            zero,
                            value,
                        );
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    UnaryOp::LogicalNot => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Eq,
                            zero,
                            value,
                        );
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                }
            }
            Expression::Binary(e1, op, e2) => {
                let v1 = self.build_expr(e1, func_data, ctx)?;
                let v2 = self.build_expr(e2, func_data, ctx)?;
                let zero = new_instr!(func_data).integer(0);
                match op {
                    BinaryOp::Add => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Add,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::Sub => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Sub,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::Mul => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Mul,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::Div => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Div,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::Rem => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Mod,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::Less => {
                        let instr =
                            func_data
                                .dfg_mut()
                                .new_value()
                                .binary(koopa::ir::BinaryOp::Lt, v1, v2);
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::Le => {
                        let instr =
                            func_data
                                .dfg_mut()
                                .new_value()
                                .binary(koopa::ir::BinaryOp::Le, v1, v2);
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::Eq => {
                        let instr =
                            func_data
                                .dfg_mut()
                                .new_value()
                                .binary(koopa::ir::BinaryOp::Eq, v1, v2);
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::Ge => {
                        let instr = new_instr!(func_data).binary(koopa::ir::BinaryOp::Ge, v1, v2);
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::Greater => {
                        let instr = new_instr!(func_data).binary(koopa::ir::BinaryOp::Gt, v1, v2);
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::NotEq => {
                        let instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v1, v2);
                        add_instr!(func_data, ctx.in_block, instr);
                        Ok(instr)
                    }
                    BinaryOp::LogicalAnd => {
                        let v1_instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v1, zero);
                        add_instr!(func_data, ctx.in_block, v1_instr);

                        let v2_instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v2, zero);
                        add_instr!(func_data, ctx.in_block, v2_instr);

                        let result = new_instr!(func_data).binary(
                            koopa::ir::BinaryOp::And,
                            v1_instr,
                            v2_instr,
                        );
                        add_instr!(func_data, ctx.in_block, result);

                        Ok(result)
                    }
                    BinaryOp::LogicalOr => {
                        let v1_instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v1, zero);
                        add_instr!(func_data, ctx.in_block, v1_instr);

                        let v2_instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v2, zero);
                        add_instr!(func_data, ctx.in_block, v2_instr);

                        let result = new_instr!(func_data).binary(
                            koopa::ir::BinaryOp::Or,
                            v1_instr,
                            v2_instr,
                        );
                        add_instr!(func_data, ctx.in_block, result);

                        Ok(result)
                    }
                }
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
}

impl SymbolValueMap {
    fn new() -> Self {
        Self {
            parent: None,
            map: HashMap::new(),
        }
    }

    /// 往当前层级添加ident-value映射
    fn add_symbol(&mut self, id: String, value: Value) {
        self.map.insert(id, value);
    }

    /// 从当前层级逐级往上查找ident
    fn find_symbol(&self, id: &str) -> Value {
        match self.map.get(id) {
            Some(val) => *val,
            None => match &self.parent {
                Some(parent) => (*parent).borrow().find_symbol(id),
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
