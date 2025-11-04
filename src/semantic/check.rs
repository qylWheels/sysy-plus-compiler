use std::{cell::RefCell, rc::Rc};

use crate::{
    parser::ast::{
        common::ResolveStatus,
        compunit::CompUnit,
        expression::{BinaryOp, Expression, UnaryOp},
        item::Item,
        statement::Statement,
    },
    semantic::symbol_table::{Qualifier, SymbolInfo, SymbolTable, SymbolTableError},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("{0:?} is not a constant expression")]
    ConstEvalError(Expression),

    #[error("{0} is not a constant identifer")]
    ConstIdentError(String),

    #[error("symbol table error: {0}")]
    SymbolTableError(#[from] SymbolTableError),

    #[error("unresolved symbol: {0}")]
    UnresolvedSymbol(String),
}

#[derive(Debug, Clone)]
pub struct SemanticChecker {
    symtable: Rc<RefCell<SymbolTable>>,
}

impl SemanticChecker {
    pub fn new() -> Self {
        Self {
            symtable: Rc::new(RefCell::new(SymbolTable::new())),
        }
    }

    pub fn get_symbol_table(&self) -> Rc<RefCell<SymbolTable>> {
        Rc::clone(&self.symtable)
    }

    // 执行语义检查
    pub fn check(&mut self, prog: &CompUnit) -> Result<(), SemanticError> {
        for item in &prog.items {
            self.check_item(item)?;
        }
        Ok(())
    }

    // FIXME: 进入函数时应进入子作用域
    fn check_item(&mut self, item: &Item) -> Result<(), SemanticError> {
        match item {
            Item::FuncDef(f) => {
                for stmt in &f.body {
                    self.check_statement(stmt)?;
                }
            }
        }

        Ok(())
    }

    fn check_statement(&mut self, stmt: &Statement) -> Result<(), SemanticError> {
        match stmt {
            Statement::ConstDecl(v) => {
                for (ty, id, expr) in v {
                    // 将id对应的作用域信息写入ast
                    *id.scope.borrow_mut() = ResolveStatus::Resolved(Rc::clone(&self.symtable));

                    // 语义检查
                    self.check_expression(expr)?;

                    // 计算常量值
                    let val = self.eval_const_val(expr)?;

                    self.symtable.borrow_mut().add_symbol(
                        id.name.clone(),
                        SymbolInfo {
                            qualifier: Qualifier::Const,
                            ty: ty.clone(),
                            const_val: Some(val),
                        },
                    )?;
                }
            }
            Statement::VarDecl(v) => {
                for (ty, id, expr_opt) in v {
                    // 将id对应的作用域信息写入ast
                    *id.scope.borrow_mut() = ResolveStatus::Resolved(Rc::clone(&self.symtable));

                    // 语义检查
                    match expr_opt {
                        Some(expr) => self.check_expression(expr)?,
                        None => (),
                    }

                    self.symtable.borrow_mut().add_symbol(
                        id.name.clone(),
                        SymbolInfo {
                            qualifier: Qualifier::Var,
                            ty: ty.clone(),
                            const_val: None,
                        },
                    )?;
                }
            }
            Statement::Assign(lval, expr) => {
                // 将id对应的作用域信息写入ast
                *lval.scope.borrow_mut() = ResolveStatus::Resolved(Rc::clone(&self.symtable));

                let symbol = self.symtable.borrow().find_symbol(&lval.name)?;
                if symbol.is_const_val() {
                    return Err(SemanticError::ConstIdentError(lval.name.clone()));
                }
                self.check_expression(expr)?;
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
                let new_scope = Rc::new(RefCell::new(self.symtable.borrow().enter_scope()));
                self.symtable = new_scope;

                // 在新作用域中进行语义检查
                for stmt in stmts {
                    self.check_statement(stmt)?;
                }

                // 返回父作用域
                let parent = self.symtable.borrow().exit_scope().unwrap();
                self.symtable = parent;
            }
        }

        Ok(())
    }

    fn eval_const_val(&self, expr: &Expression) -> Result<i32, SemanticError> {
        match expr {
            Expression::IntLit(i) => Ok(*i),
            Expression::Ident(id) => {
                let syminfo = self.symtable.borrow().find_symbol(&id.name)?;
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
        }
    }

    fn check_expression(&mut self, expr: &Expression) -> Result<(), SemanticError> {
        match expr {
            Expression::IntLit(_) => Ok(()),
            Expression::Ident(id) => {
                // 将id对应的作用域信息写入ast
                *id.scope.borrow_mut() = ResolveStatus::Resolved(Rc::clone(&self.symtable));

                let _ = self.symtable.borrow().find_symbol(&id.name)?;
                Ok(())
            }
            Expression::Unary(_, expr) => self.check_expression(&expr),
            Expression::Binary(lhs, _, rhs) => {
                self.check_expression(&lhs)?;
                self.check_expression(&rhs)?;
                Ok(())
            }
        }
    }

    fn i32_to_bool(i: i32) -> bool {
        i != 0
    }
}
