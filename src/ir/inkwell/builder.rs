use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::{Builder, BuilderError};
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, ValueKind,
};
use inkwell::IntPredicate;
use thiserror::Error;

use crate::ir::inkwell::typemap::TypeMapper;
use crate::parser::ast::compile_unit::CompileUnit;
use crate::parser::ast::expression::{BinaryOp, Expression, UnaryOp};
use crate::parser::ast::item::Item;
use crate::parser::ast::statement::Statement;
use crate::semantic::symbol_table::{SymbolInfoError, SymbolTableError};

// 获取当前指针所在的bb
macro_rules! get_curr_bb {
    ($builder:expr) => {
        $builder.get_insert_block().unwrap()
    };
}

// 检查bb中的最后一句是不是终止语句
macro_rules! is_last_instr_terminator {
    ($bb:expr) => {
        $bb.get_last_instruction()
            .is_some_and(|instr| instr.is_terminator())
    };
}

// // 判断当前bb中最后一句是否为终止语句。若不是，添加语句；若是，不添加语句
// macro_rules! append_instr_when_nonterminator {
//     ($builder:expr, $instr:expr) => {
//         if is_last_instr_terminator!(get_curr_bb!($builder)) {
//             // 不添加语句
//             false
//         } else {
//             // 添加语句
//             $builder.insert_instruction(instr, "name");
//             true
//         }
//     };
// }

#[derive(Debug, Clone)]
struct IrGeneratorContext<'ctx> {
    /// 当前所处循环的起始bb
    curr_loop_start_bb: Option<BasicBlock<'ctx>>,

    /// 当前所处循环的末尾bb（merge块）
    curr_loop_end_bb: Option<BasicBlock<'ctx>>,
}

#[derive(Debug, Error)]
pub enum IrGeneratorError {
    #[error("symbol table error: {0}")]
    SymbolTableError(#[from] SymbolTableError),

    #[error("symbol information error: {0}")]
    SymbolInfoError(#[from] SymbolInfoError),

    #[error("llvm builder error: {0}")]
    LlvmBuilderError(#[from] BuilderError),

    #[error("scope error: {0}")]
    ScopeError(#[from] ScopeError),
}

#[derive(Debug)]
pub struct IrGenerator<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    type_mapper: TypeMapper<'ctx>,
    scope: RefCell<Scope<'ctx>>,
}

impl<'ctx> IrGenerator<'ctx> {
    pub fn new(context: &'ctx Context) -> Self {
        let module = context.create_module("main");
        let builder = context.create_builder();
        Self {
            context,
            module,
            builder,
            type_mapper: TypeMapper::new(),
            scope: RefCell::new(Scope::new()),
        }
    }

    /// 扫描所有bb，若存在有bb没有指令，则断定其为不可达，并插入一条unreachable指令
    fn scan_and_insert_unreachable(&self) -> Result<(), IrGeneratorError> {
        for func in self.module.get_functions() {
            for bb in func.get_basic_blocks() {
                if bb.get_first_instruction().is_none() {
                    self.builder.build_unreachable()?;
                }
            }
        }
        Ok(())
    }

    pub fn build_compunit(&self, prog: &CompileUnit) -> Result<Module<'ctx>, IrGeneratorError> {
        // TODO: 编译器内置item

        // 用户写的item
        for item in &prog.items {
            self.build_item(item)?;
        }

        Ok(self.module.clone())
    }

    fn build_item(&self, item: &Item) -> Result<(), IrGeneratorError> {
        match item {
            Item::FuncDef(f) => {
                let (id, fparams, ret_ty, body) = (&f.ident, &f.fparams, &f.return_type, &f.body);

                // 构建参数类型
                let params_tys = fparams
                    .iter()
                    .map(|(ty, _)| self.type_mapper.map(ty, self.context).into())
                    .collect::<Vec<BasicMetadataTypeEnum<'_>>>();

                // 构建返回值类型
                let ret_llvm_ty = self.type_mapper.map(ret_ty, self.context);

                // 构建函数类型
                let fn_llvm_ty = ret_llvm_ty.fn_type(&params_tys, false);

                // 创建函数
                let func = self.module.add_function(&id.name, fn_llvm_ty, None);

                // 将函数参数加入值作用域
                let params = func.get_params();
                for i in 0..params.len() {
                    self.scope
                        .borrow_mut()
                        .add_value(fparams[i].1.name.clone(), &params[i]);
                }

                // 创建函数入口块
                let entry_bb = self.context.append_basic_block(func, "entry");
                self.builder.position_at_end(entry_bb);

                // 加入scope
                // XXX: 一定要在此处加入以支持递归
                self.scope.borrow_mut().add_function(id.name.clone(), &func);

                // 生成函数内部的语句
                for stmt in body {
                    self.build_stmt(
                        stmt,
                        &func,
                        IrGeneratorContext {
                            curr_loop_start_bb: None,
                            curr_loop_end_bb: None,
                        },
                    )?;
                }

                // 为所有空块插入unreachable指令
                self.scan_and_insert_unreachable()?;

                // 验证函数正确性
                func.verify(true);

                Ok(())
            }
            Item::GlobalVar(stmt) => match stmt {
                Statement::VarDecl(v) => {
                    todo!()
                    // for (ty, id, expr_opt) in v {
                    //     match expr_opt {
                    //         Some(expr) => {
                    //             let value = self.build_expr(expr)?;
                    //         }
                    //         None => {}
                    //     }
                    // }
                }
                Statement::ConstDecl(c) => {
                    todo!()
                }
                _ => unimplemented!(),
            },
        }
    }

    fn build_stmt(
        &self,
        stmt: &Statement,
        func: &FunctionValue,
        irgen_ctx: IrGeneratorContext,
    ) -> Result<(), IrGeneratorError> {
        match stmt {
            Statement::Return(expr) => {
                if is_last_instr_terminator!(get_curr_bb!(self.builder)) {
                    return Ok(());
                }
                let v = self.build_expr(expr)?;
                let v = if v.is_pointer_value() {
                    self.builder
                        .build_load(self.context.i32_type(), v.into_pointer_value(), "")
                        .unwrap()
                } else {
                    v
                };
                self.builder.build_return(Some(&v))?;
            }
            Statement::ConstDecl(v) => {
                for (ty, id, expr) in v {
                    let llvm_ty = self.type_mapper.map(ty, self.context);
                    let alloca = self.builder.build_alloca(llvm_ty, &id.name)?;
                    let v = self.build_expr(expr)?;
                    self.builder.build_store(alloca, v)?;
                    self.scope
                        .borrow_mut()
                        .add_value(id.name.clone(), &alloca.as_basic_value_enum());
                }
            }
            Statement::VarDecl(v) => {
                for (ty, id, expr_opt) in v {
                    let llvm_ty = self.type_mapper.map(ty, self.context);
                    let alloca = self.builder.build_alloca(llvm_ty, &id.name)?;
                    let v = match expr_opt {
                        Some(expr) => self.build_expr(expr)?,
                        None => self
                            .context
                            .i32_type()
                            .const_int(0, false)
                            .as_basic_value_enum(),
                    };
                    self.builder.build_store(alloca, v)?;
                    self.scope
                        .borrow_mut()
                        .add_value(id.name.clone(), &alloca.as_basic_value_enum());
                }
            }
            Statement::Assign(id, expr) => {
                let ptr = self.scope.borrow().find_value(&id.name).unwrap();
                let v = self.build_expr(expr)?;
                self.builder.build_store(ptr.into_pointer_value(), v)?;
            }
            Statement::Expression(expr_opt) => match expr_opt {
                Some(expr) => {
                    self.build_expr(expr)?;
                }
                None => {}
            },
            Statement::Block(stmts) => {
                let new_scope = self.scope.borrow().enter_scope();
                *self.scope.borrow_mut() = new_scope;

                for stmt in stmts {
                    self.build_stmt(stmt, func, irgen_ctx.clone())?;
                }

                let old_scope = self.scope.borrow().exit_scope()?;
                *self.scope.borrow_mut() = old_scope;
            }
            Statement::If(cond, then_br, else_br_opt) => {
                // 构建基本块
                let then_bb = self.context.append_basic_block(func.clone(), "then");
                let else_bb = self.context.append_basic_block(func.clone(), "else");
                let merge_bb = self.context.append_basic_block(func.clone(), "merge");

                // 根据cond的值决定跳转方向
                let cond_v = self.build_expr(cond)?;
                if !is_last_instr_terminator!(get_curr_bb!(self.builder)) {
                    self.builder.build_conditional_branch(
                        cond_v.into_int_value(),
                        then_bb,
                        else_bb,
                    )?;
                }

                // then块
                self.builder.position_at_end(then_bb);
                self.build_stmt(then_br, func, irgen_ctx.clone())?;
                if !is_last_instr_terminator!(get_curr_bb!(self.builder)) {
                    self.builder.build_unconditional_branch(merge_bb)?;
                }

                // else块
                self.builder.position_at_end(else_bb);
                match else_br_opt {
                    Some(else_br) => {
                        self.build_stmt(else_br, func, irgen_ctx.clone())?;
                        if !is_last_instr_terminator!(get_curr_bb!(self.builder)) {
                            self.builder.build_unconditional_branch(merge_bb)?;
                        }
                    }
                    None => {
                        // else块为空，需要补一个跳转到merge_bb的语句
                        let _ = self.builder.build_unconditional_branch(merge_bb)?;
                    }
                }

                // merge块
                self.builder.position_at_end(merge_bb);
            }
            Statement::While(cond, stmt) => {
                // 构建基本块
                let cond_bb = self.context.append_basic_block(func.clone(), "cond");
                let body_bb = self.context.append_basic_block(func.clone(), "body");
                let merge_bb = self.context.append_basic_block(func.clone(), "merge");

                // 由前序代码跳转到cond_bb
                if !is_last_instr_terminator!(get_curr_bb!(self.builder)) {
                    self.builder.build_unconditional_branch(cond_bb)?;
                }

                // 根据cond的值决定跳转方向
                self.builder.position_at_end(cond_bb);
                let cond_v = self.build_expr(cond)?;
                self.builder.build_conditional_branch(
                    cond_v.into_int_value(),
                    body_bb,
                    merge_bb,
                )?;

                // body块
                self.builder.position_at_end(body_bb);
                self.build_stmt(
                    stmt,
                    func,
                    IrGeneratorContext {
                        curr_loop_start_bb: Some(cond_bb),
                        curr_loop_end_bb: Some(merge_bb),
                    },
                )?;
                if get_curr_bb!(self.builder).get_last_instruction().is_none() {
                    // 空的body块，需要补一条跳到cond块的指令
                    self.builder.build_unconditional_branch(cond_bb)?;
                } else {
                    if !is_last_instr_terminator!(get_curr_bb!(self.builder)) {
                        self.builder.build_unconditional_branch(cond_bb)?;
                    }
                }

                // merge块
                self.builder.position_at_end(merge_bb);
            }
            Statement::Break => {
                if !is_last_instr_terminator!(get_curr_bb!(self.builder)) {
                    self.builder
                        .build_unconditional_branch(irgen_ctx.curr_loop_end_bb.unwrap())?;
                }
            }
            Statement::Continue => {
                if !is_last_instr_terminator!(get_curr_bb!(self.builder)) {
                    self.builder
                        .build_unconditional_branch(irgen_ctx.curr_loop_start_bb.unwrap())?;
                }
            }
        }

        Ok(())
    }

    fn build_expr(&self, expr: &Expression) -> Result<BasicValueEnum<'ctx>, IrGeneratorError> {
        match expr {
            Expression::IntLit(i) => Ok(self
                .context
                .i32_type()
                .const_int(*i as u64, true)
                .as_basic_value_enum()),
            Expression::Ident(id) => {
                let val = self.scope.borrow().find_value(&id.name);
                if val == None {
                    let func = self.scope.borrow().find_function(&id.name).unwrap();
                    Ok(func
                        .as_global_value()
                        .as_pointer_value()
                        .as_basic_value_enum())
                } else {
                    Ok(val.unwrap())
                }
            }
            Expression::Unary(op, expr) => {
                let v = self.build_expr(expr)?;
                match op {
                    UnaryOp::Plus => Ok(v),
                    UnaryOp::Minus => {
                        let result = self
                            .builder
                            .build_int_sub(
                                self.context.i32_type().const_int(0, false),
                                v.into_int_value(),
                                "",
                            )?
                            .as_basic_value_enum();
                        Ok(result)
                    }
                    UnaryOp::LogicalNot => {
                        let result = self
                            .builder
                            .build_not(v.into_int_value(), "")?
                            .as_basic_value_enum();
                        Ok(result)
                    }
                }
            }
            Expression::Binary(lhs, op, rhs) => {
                let lhs_v = self.build_expr(lhs)?;
                let rhs_v = self.build_expr(rhs)?;

                // 或lhs_v或rhs_v为指针，则先从中读出实际存储的值
                let (lhs_v, rhs_v) = match op {
                    BinaryOp::LogicalAnd | BinaryOp::LogicalOr => {
                        let lhs_real = if lhs_v.is_pointer_value() {
                            self.builder
                                .build_load(
                                    self.context.bool_type(),
                                    lhs_v.into_pointer_value(),
                                    "",
                                )?
                                .as_basic_value_enum()
                        } else {
                            lhs_v
                        };

                        let rhs_real = if rhs_v.is_pointer_value() {
                            self.builder
                                .build_load(
                                    self.context.bool_type(),
                                    rhs_v.into_pointer_value(),
                                    "",
                                )?
                                .as_basic_value_enum()
                        } else {
                            rhs_v
                        };

                        (lhs_real, rhs_real)
                    }

                    _ => {
                        let lhs_real = if lhs_v.is_pointer_value() {
                            self.builder
                                .build_load(
                                    self.context.i32_type(),
                                    lhs_v.into_pointer_value(),
                                    "",
                                )?
                                .as_basic_value_enum()
                        } else {
                            lhs_v
                        };

                        let rhs_real = if rhs_v.is_pointer_value() {
                            self.builder
                                .build_load(
                                    self.context.i32_type(),
                                    rhs_v.into_pointer_value(),
                                    "",
                                )?
                                .as_basic_value_enum()
                        } else {
                            rhs_v
                        };

                        (lhs_real, rhs_real)
                    }
                };

                match op {
                    // 算术运算符
                    BinaryOp::Add => {
                        let result = self.builder.build_int_add(
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::Sub => {
                        let result = self.builder.build_int_sub(
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::Mul => {
                        let result = self.builder.build_int_mul(
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::Div => {
                        let result = self.builder.build_int_signed_div(
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::Rem => {
                        let result = self.builder.build_int_signed_rem(
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }

                    // 比较运算符
                    BinaryOp::Less => {
                        let result = self.builder.build_int_compare(
                            IntPredicate::SLT,
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::Le => {
                        let result = self.builder.build_int_compare(
                            IntPredicate::SLE,
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::Eq => {
                        let result = self.builder.build_int_compare(
                            IntPredicate::EQ,
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::Ge => {
                        let result = self.builder.build_int_compare(
                            IntPredicate::SGE,
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::Greater => {
                        let result = self.builder.build_int_compare(
                            IntPredicate::SGT,
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::NotEq => {
                        let result = self.builder.build_int_compare(
                            IntPredicate::NE,
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }

                    // 逻辑运算符
                    // TODO: 实现短路求值
                    BinaryOp::LogicalAnd => {
                        let result = self.builder.build_and(
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                    BinaryOp::LogicalOr => {
                        let result = self.builder.build_or(
                            lhs_v.into_int_value(),
                            rhs_v.into_int_value(),
                            "",
                        )?;
                        Ok(result.as_basic_value_enum())
                    }
                }
            }
            Expression::Call(fn_name, args) => {
                let func = self.scope.borrow().find_function(&fn_name.name).unwrap();
                let args = args
                    .iter()
                    .map(|arg| self.build_expr(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                let args: Vec<BasicMetadataValueEnum<'_>> =
                    args.iter().map(|arg| arg.clone().into()).collect();
                let result = self
                    .builder
                    .build_call(func, &args, "")?
                    .try_as_basic_value();
                if let ValueKind::Basic(val) = result {
                    Ok(val)
                } else {
                    // 若函数返回值为空，则返回一个空结构体（元组）
                    Ok(self
                        .context
                        .struct_type(&[], true)
                        .const_named_struct(&[])
                        .as_basic_value_enum())
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ScopeError {
    #[error("no parent scope")]
    NoParentScope,
}

#[derive(Debug, Clone)]
struct Scope<'ctx> {
    parent: Option<Rc<RefCell<Self>>>,

    /// 值作用域
    val_scope: HashMap<String, BasicValueEnum<'ctx>>,

    /// 函数作用域
    func_scope: HashMap<String, FunctionValue<'ctx>>,
}

impl<'ctx> Scope<'ctx> {
    fn new() -> Self {
        Self {
            parent: None,
            val_scope: HashMap::new(),
            func_scope: HashMap::new(),
        }
    }

    /// 往当前层级添加ident-val映射
    fn add_value(&mut self, id: String, value: &BasicValueEnum<'ctx>) {
        self.val_scope.insert(id, value.clone());
    }

    /// 从当前层级逐级往上查找ident
    fn find_value(&self, id: &str) -> Option<BasicValueEnum<'ctx>> {
        match self.val_scope.get(id) {
            Some(val) => Some(val.clone()),
            None => match &self.parent {
                Some(parent) => (*parent).borrow().find_value(id),
                None => unreachable!(),
            },
        }
    }

    /// 往当前层级添加ident-function映射
    fn add_function(&mut self, id: String, func: &FunctionValue<'ctx>) {
        self.func_scope.insert(id, func.clone());
    }

    /// 从当前层级逐级往上查找ident
    fn find_function(&self, id: &str) -> Option<FunctionValue<'ctx>> {
        match self.func_scope.get(id) {
            Some(func) => Some(func.clone()),
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

    fn exit_scope(&self) -> Result<Self, ScopeError> {
        match self.parent.as_ref() {
            Some(parent) => Ok((*parent).borrow().clone()),
            None => Err(ScopeError::NoParentScope),
        }
    }
}
