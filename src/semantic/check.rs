use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
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
    #[error("{0:?} is not a constant expression")]
    ConstEvalError(Expression),

    #[error("{0} is not a constant identifer")]
    ConstIdentError(String),

    #[error("unresolved symbol: {0}")]
    UnresolvedSymbol(String),

    #[error("\"break\" is not in while statement")]
    BreakError,

    #[error("\"continue\" is not in while statement")]
    ContinueError,

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

    // 执行语义检查
    pub fn check(&mut self, prog: &CompileUnit) -> Result<(), SemanticError> {
        for item in &prog.items {
            self.check_item(item)?;
        }
        Ok(())
    }

    fn check_item(&mut self, item: &Item) -> Result<(), SemanticError> {
        match item {
            Item::FuncDef(f) => {
                // 对函数名称进行检查
                let id = f.ident.name.clone();
                // FIXME: 这里先用一个占位的SymbolInfo哄类型检查器，后续可能要斟酌其字段该如何设置
                self.symtable.add_symbol(
                    id,
                    SymbolInfo {
                        qualifier: Qualifier::Const,
                        ty: Type::Void,
                        const_val: None,
                    },
                )?;

                // 创建子作用域
                let new_scope = self.symtable.enter_scope();
                self.symtable = new_scope;

                // 对函数形参进行检查
                for (ty, id) in &f.fparams {
                    self.symtable.add_symbol(
                        id.name.clone(),
                        SymbolInfo {
                            qualifier: Qualifier::Var,
                            ty: ty.clone(),
                            const_val: None,
                        },
                    )?;
                }

                // 对函数体内部语句进行语义检查
                for stmt in &f.body {
                    self.check_statement(stmt)?;
                }

                // 返回父作用域
                let old_scope = self.symtable.exit_scope()?;
                self.symtable = old_scope;
            }
        }

        Ok(())
    }

    fn check_statement(&mut self, stmt: &Statement) -> Result<(), SemanticError> {
        match stmt {
            Statement::ConstDecl(v) => {
                for (ty, id, expr) in v {
                    // 对等号右边的表达式进行语义检查
                    self.check_expression(expr)?;

                    // 计算常量值
                    let val = self.eval_const_val(expr)?;

                    // 创建SymbolInfo结构体
                    let syminfo = SymbolInfo {
                        qualifier: Qualifier::Const,
                        ty: ty.clone(),
                        const_val: Some(val),
                    };

                    // 将id对应的信息写入ast
                    *id.resolve_status.borrow_mut() = ResolveStatus::Resolved(syminfo.clone());

                    // 将id对应的信息写入符号表
                    self.symtable.add_symbol(id.name.clone(), syminfo.clone())?;
                }
            }
            Statement::VarDecl(v) => {
                for (ty, id, expr_opt) in v {
                    // 对等号右边的表达式进行语义检查
                    match expr_opt {
                        Some(expr) => self.check_expression(expr)?,
                        None => (),
                    }

                    // 创建SymbolInfo结构体
                    let syminfo = SymbolInfo {
                        qualifier: Qualifier::Var,
                        ty: ty.clone(),
                        const_val: None,
                    };

                    // 将id对应的作用域信息写入ast
                    *id.resolve_status.borrow_mut() = ResolveStatus::Resolved(syminfo.clone());

                    // 将id对应的作用域信息写入符号表
                    self.symtable.add_symbol(id.name.clone(), syminfo.clone())?;
                }
            }
            Statement::Assign(lval, expr) => {
                // 在符号表中查找id对应的SymbolInfo
                let syminfo = self.symtable.find_symbol(&lval.name)?;
                if syminfo.is_const_val() {
                    return Err(SemanticError::ConstIdentError(lval.name.clone()));
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

    fn eval_const_val(&self, expr: &Expression) -> Result<i32, SemanticError> {
        match expr {
            Expression::IntLit(i) => Ok(*i),
            Expression::Ident(id) => {
                let syminfo = self.symtable.find_symbol(&id.name)?;
                syminfo
                    .const_val
                    .ok_or(SemanticError::ConstEvalError(expr.clone()))
            }
            Expression::Unary(op, expr) => match op {
                UnaryOp::Plus => self.eval_const_val(expr),
                UnaryOp::Minus => self.eval_const_val(expr).map(|val| -val),
                UnaryOp::LogicalNot => self.eval_const_val(expr).map(|val| !val),
            },
            Expression::Binary(lhs, op, rhs) => match op {
                // 算数运算符
                BinaryOp::Add => Ok(self.eval_const_val(&lhs)? + self.eval_const_val(&rhs)?),
                BinaryOp::Sub => Ok(self.eval_const_val(&lhs)? - self.eval_const_val(&rhs)?),
                BinaryOp::Mul => Ok(self.eval_const_val(&lhs)? * self.eval_const_val(&rhs)?),
                BinaryOp::Div => Ok(self.eval_const_val(&lhs)? / self.eval_const_val(&rhs)?),
                BinaryOp::Rem => Ok(self.eval_const_val(&lhs)? % self.eval_const_val(&rhs)?),

                // 比较运算符
                BinaryOp::Less => {
                    Ok((self.eval_const_val(&lhs)? < self.eval_const_val(&rhs)?).into())
                }
                BinaryOp::Le => {
                    Ok((self.eval_const_val(&lhs)? <= self.eval_const_val(&rhs)?).into())
                }
                BinaryOp::Eq => {
                    Ok((self.eval_const_val(&lhs)? == self.eval_const_val(&rhs)?).into())
                }
                BinaryOp::Ge => {
                    Ok((self.eval_const_val(&lhs)? >= self.eval_const_val(&rhs)?).into())
                }
                BinaryOp::Greater => {
                    Ok((self.eval_const_val(&lhs)? > self.eval_const_val(&rhs)?).into())
                }
                BinaryOp::NotEq => {
                    Ok((self.eval_const_val(&lhs)? != self.eval_const_val(&rhs)?).into())
                }

                // 逻辑运算符
                BinaryOp::LogicalAnd => Ok((Self::i32_to_bool(self.eval_const_val(&lhs)?)
                    && Self::i32_to_bool(self.eval_const_val(&rhs)?))
                .into()),
                BinaryOp::LogicalOr => Ok((Self::i32_to_bool(self.eval_const_val(&lhs)?)
                    || Self::i32_to_bool(self.eval_const_val(&rhs)?))
                .into()),
            },
            Expression::Call(identifier, expressions) => todo!(),
        }
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
            Expression::Call(identifier, expressions) => todo!(),
        }
    }

    fn i32_to_bool(i: i32) -> bool {
        i != 0
    }
}
