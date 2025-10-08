use crate::parser::ast::{
    common::*,
    compunit::CompUnit,
    expression::{BinaryOp, Expression, UnaryOp},
    item::Item,
    statement::Statement,
};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SymbolTableError {
    #[error("{0:?} is not a constant expression")]
    ConstEvalError(Expression),

    #[error("{0} is not exists in symbol table")]
    SymbolNotFound(Ident),
}

#[derive(Debug, Clone)]
pub struct SymbolTable {
    map: HashMap<Ident, SymbolInfo>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn check(&mut self, prog: &CompUnit) -> Result<(), SymbolTableError> {
        for item in &prog.items {
            self.check_item(item)?;
        }

        Ok(())
    }

    fn check_item(&mut self, item: &Item) -> Result<(), SymbolTableError> {
        match item {
            Item::FuncDef(f) => {
                for stmt in &f.body {
                    self.check_statement(stmt)?;
                }
            }
        }

        Ok(())
    }

    fn check_statement(&mut self, stmt: &Statement) -> Result<(), SymbolTableError> {
        match stmt {
            Statement::ConstDecl(v) => {
                for (ty, id, expr) in v {
                    let val = self.eval_const_val(expr)?;
                    self.map.insert(
                        id.clone(),
                        SymbolInfo {
                            qualifier: Qualifier::Const,
                            ty: ty.clone(),
                            const_val: Some(val),
                        },
                    );
                }
            }
            Statement::VarDecl(v) => {
                for (ty, id, _) in v {
                    self.map.insert(
                        id.clone(),
                        SymbolInfo {
                            qualifier: Qualifier::Var,
                            ty: ty.clone(),
                            const_val: None,
                        },
                    );
                }
            }
            Statement::Assign(lval, expr) => {
                self.find_symbol(lval)?;
                self.check_expression(expr)?;
            }
            Statement::Return(expr) => {
                self.check_expression(expr)?;
            }
        }

        Ok(())
    }

    fn eval_const_val(&self, expr: &Expression) -> Result<i32, SymbolTableError> {
        match expr {
            Expression::IntLit(i) => Ok(*i),
            Expression::Ident(id) => {
                let syminfo = self.find_symbol(id)?;
                syminfo
                    .const_val
                    .ok_or(SymbolTableError::ConstEvalError(expr.clone()))
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

    fn check_expression(&mut self, expr: &Expression) -> Result<(), SymbolTableError> {
        match expr {
            Expression::IntLit(_) => Ok(()),
            Expression::Ident(id) => Ok(self.find_symbol(id).map(|_| ())?),
            Expression::Unary(_, expr) => self.check_expression(&expr),
            Expression::Binary(lhs, _, rhs) => {
                self.check_expression(&lhs)?;
                self.check_expression(&rhs)?;
                Ok(())
            }
        }
    }

    fn find_symbol(&self, symbol: &Ident) -> Result<&SymbolInfo, SymbolTableError> {
        self.map
            .get(symbol)
            .ok_or(SymbolTableError::SymbolNotFound(symbol.clone()))
    }

    fn i32_to_bool(i: i32) -> bool {
        i != 0
    }
}

#[derive(Debug, Clone)]
struct SymbolInfo {
    qualifier: Qualifier,
    ty: Type,
    const_val: Option<i32>,
}

#[derive(Debug, Clone, Copy)]
enum Qualifier {
    Var,
    Const,
}
