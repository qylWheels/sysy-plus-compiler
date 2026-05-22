use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    builtins::builtin_functions::get_builtin_functions,
    parser::ast::{
        common::{ResolveStatus, Type},
        compile_unit::CompileUnit,
        expression::{BinaryOp, Expression, UnaryOp},
        item::Item,
        statement::Statement,
    },
    semantic::symbol_table::{Qualifier, SymbolInfo, SymbolTable, SymbolTableError},
};
use thiserror::Error;

/// 记录当前while的深度，用于判断break和continue是否在while里
static WHILE_DEPTH_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("unresolved symbol: {0}")]
    UnresolvedSymbol(String),

    #[error("\"break\" is not in while statement")]
    BreakError,

    #[error("\"continue\" is not in while statement")]
    ContinueError,

    #[error("expected {0} arguments, found {1}")]
    ArgumentCountError(usize, usize),

    #[error("expected type \"{0:?}\", found \"{1:?}\"")]
    TypeError(Type, Type),

    #[error("constant value expected")]
    ConstValueError,

    #[error("symbol table error: {0}")]
    SymbolTableError(#[from] SymbolTableError),
}

#[derive(Debug, Clone)]
pub struct SemanticChecker {
    symtable: SymbolTable,
}

impl SemanticChecker {
    pub fn new() -> Self {
        Self {
            symtable: SymbolTable::new(),
        }
    }

    /// 执行语义检查
    pub fn check(&mut self, prog: &CompileUnit) -> Result<(), SemanticError> {
        // 提前将sysy运行时库的符号加入符号表中
        let sysy_runtime_lib_guard = get_builtin_functions().lock().unwrap();
        for (id, syminfo) in sysy_runtime_lib_guard.iter() {
            self.symtable.add_symbol(id.clone(), syminfo.clone())?;
        }

        let new_scope = self.symtable.enter_scope();
        self.symtable = new_scope;

        // 开始对整个程序的检查
        for item in &prog.items {
            self.check_item(item)?;
        }

        let old_scope = self.symtable.exit_scope()?;
        self.symtable = old_scope;
        Ok(())
    }

    fn check_item(&mut self, item: &Item) -> Result<(), SemanticError> {
        match item {
            Item::FuncDef(f) => {
                // 对函数名称进行检查并将id对应的信息写入ast
                let id = f.ident.name.clone();
                let params_types = f
                    .fparams
                    .iter()
                    .map(|(_, ty)| Box::new(ty.clone()))
                    .collect::<Vec<_>>();
                let syminfo = SymbolInfo {
                    qualifier: Qualifier::Const, // TODO: 斟酌此字段的内容
                    ty: Type::Function(params_types, Box::new(f.return_type.clone())),
                };
                self.symtable.add_symbol(id, syminfo.clone())?;
                *f.ident.resolve_status.borrow_mut() = ResolveStatus::Resolved(syminfo.clone());

                // 创建子作用域
                let new_scope = self.symtable.enter_scope();
                self.symtable = new_scope;

                // 对函数形参进行检查并将信息写入ast
                for (id, ty) in &f.fparams {
                    let syminfo = SymbolInfo {
                        qualifier: Qualifier::Var,
                        ty: ty.clone(),
                    };
                    self.symtable.add_symbol(id.name.clone(), syminfo.clone())?;
                    *id.resolve_status.borrow_mut() = ResolveStatus::Resolved(syminfo.clone());
                }

                // 对函数体内部语句进行语义检查
                for stmt in &f.body {
                    self.check_statement(stmt)?;
                }

                // 返回父作用域
                let old_scope = self.symtable.exit_scope()?;
                self.symtable = old_scope;
            }
            Item::GlobalVar(stmt) => {
                self.check_statement(stmt)?;
            }
        }

        Ok(())
    }

    fn check_statement(&mut self, stmt: &Statement) -> Result<(), SemanticError> {
        match stmt {
            Statement::ConstDef(id, ty_opt, expr) => {
                // 对等号右边的表达式进行语义检查
                self.check_expression(expr)?;

                // 创建SymbolInfo结构体
                let syminfo = SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: ty_opt.clone().unwrap(),
                };

                // 将id对应的信息写入ast
                *id.resolve_status.borrow_mut() = ResolveStatus::Resolved(syminfo.clone());

                // 将id对应的信息写入符号表
                self.symtable.add_symbol(id.name.clone(), syminfo.clone())?;
            }
            Statement::VarDef(id, ty_opt, expr) => {
                // 对等号右边的表达式进行语义检查
                self.check_expression(expr)?;

                // 创建SymbolInfo结构体
                let syminfo = SymbolInfo {
                    qualifier: Qualifier::Var,
                    ty: ty_opt.clone().unwrap(),
                };

                // 将id对应的作用域信息写入ast
                // dbg!(id);
                *id.resolve_status.borrow_mut() = ResolveStatus::Resolved(syminfo.clone());
                // dbg!(id);

                // 将id对应的作用域信息写入符号表
                self.symtable.add_symbol(id.name.clone(), syminfo.clone())?;
            }
            Statement::Assign(lval, expr) => {
                // 在符号表中查找id对应的SymbolInfo
                let syminfo = self.symtable.find_symbol(&lval.name)?;
                if syminfo.is_const_val() {
                    return Err(SemanticError::ConstValueError);
                }
                self.check_expression(expr)?;

                // 将id对应的作用域信息写入ast
                *lval.resolve_status.borrow_mut() = ResolveStatus::Resolved(syminfo);
            }
            Statement::Return(expr) => {
                self.check_expression(expr)?;
            }
            Statement::Expression(expr) => match expr {
                Some(expr) => self.check_expression(expr)?,
                None => (),
            },
            Statement::Block(stmts) => {
                // 开辟新作用域
                let new_scope = self.symtable.enter_scope();
                self.symtable = new_scope;

                // 在新作用域中进行语义检查
                for stmt in stmts {
                    self.check_statement(stmt)?;
                }

                // 返回父作用域
                let parent = self.symtable.exit_scope()?;
                self.symtable = parent;
            }
            Statement::If(guard, then_branch, else_branch) => {
                // 检查条件
                self.check_expression(guard)?;

                // 检查then分支
                self.symtable = self.symtable.enter_scope();
                self.check_statement(&then_branch)?;
                self.symtable = self.symtable.exit_scope()?;

                // 检查else分支
                match else_branch {
                    Some(else_branch) => {
                        self.symtable = self.symtable.enter_scope();
                        self.check_statement(&else_branch)?;
                        self.symtable = self.symtable.exit_scope()?;
                    }
                    None => (),
                }
            }
            Statement::While(guard, stmt) => {
                // 检查条件
                self.check_expression(guard)?;

                // 检查内部语句
                self.symtable = self.symtable.enter_scope();
                WHILE_DEPTH_COUNTER.fetch_add(1, Ordering::Relaxed); // 进入内部，深度加1
                self.check_statement(stmt)?;
                WHILE_DEPTH_COUNTER.fetch_sub(1, Ordering::Relaxed); // 退回外部，深度减1
                self.symtable = self.symtable.exit_scope()?;
            }
            Statement::Break => {
                if WHILE_DEPTH_COUNTER.load(Ordering::Relaxed) == 0 {
                    return Err(SemanticError::BreakError);
                }
            }
            Statement::Continue => {
                if WHILE_DEPTH_COUNTER.load(Ordering::Relaxed) == 0 {
                    return Err(SemanticError::ContinueError);
                }
            }
        }

        Ok(())
    }

    fn check_expression(&mut self, expr: &Expression) -> Result<(), SemanticError> {
        match expr {
            Expression::IntLit(_) => Ok(()),
            Expression::Ident(id) => {
                // 查找id对应的SymbolInfo
                let syminfo = self.symtable.find_symbol(&id.name)?;

                // 将id对应的作用域信息写入ast
                *id.resolve_status.borrow_mut() = ResolveStatus::Resolved(syminfo);

                Ok(())
            }
            Expression::Unary(_, expr) => self.check_expression(&expr),
            Expression::Binary(lhs, _, rhs) => {
                self.check_expression(&lhs)?;
                self.check_expression(&rhs)?;
                Ok(())
            }
            Expression::Call(id, exprs) => {
                // 检查调用的函数是否存在
                let syminfo = self.symtable.find_symbol(&id.name)?;

                // 检查实参个数是否等于形参个数
                match syminfo.ty {
                    Type::Function(params, _ret) => {
                        if exprs.len() != params.len() {
                            return Err(SemanticError::ArgumentCountError(
                                params.len(),
                                exprs.len(),
                            ));
                        }
                    }
                    _ => unreachable!(),
                }

                // 检查参数类型
                for expr in exprs {
                    self.check_expression(expr)?;
                }

                Ok(())
            }
        }
    }

    fn i32_to_bool(i: i32) -> bool {
        i != 0
    }
}
