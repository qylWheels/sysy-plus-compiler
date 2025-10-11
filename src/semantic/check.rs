use crate::{
    parser::ast::{
        common::Ident,
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
    ConstIdentError(Ident),

    #[error("{0} is not exists in symbol table")]
    SymbolNotFound(Ident),
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

    pub fn get_symbol_table(&self) -> &SymbolTable {
        &self.symtable
    }

    // 执行语义检查
    pub fn check(&mut self, prog: &CompUnit) -> Result<(), SemanticError> {
        for item in &prog.items {
            self.check_item(item)?;
        }
        Ok(())
    }

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
                    let val = self.eval_const_val(expr)?;
                    self.symtable.add_symbol(
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
                    self.symtable.add_symbol(
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
                self.symtable.find_symbol(lval).map_err(|err| match err {
                    SymbolTableError::SymbolNotFound(id) => SemanticError::SymbolNotFound(id),
                })?;
                self.check_expression(expr)?;
            }
            Statement::Return(expr) => {
                self.check_expression(expr)?;
            }
        }

        Ok(())
    }

    fn eval_const_val(&self, expr: &Expression) -> Result<i32, SemanticError> {
        match expr {
            Expression::IntLit(i) => Ok(*i),
            Expression::Ident(id) => {
                let syminfo = self.symtable.find_symbol(id).map_err(|err| match err {
                    SymbolTableError::SymbolNotFound(id) => SemanticError::SymbolNotFound(id),
                })?;
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
                Ok(self
                    .symtable
                    .find_symbol(id)
                    .map(|_| ())
                    .map_err(|err| match err {
                        SymbolTableError::SymbolNotFound(id) => SemanticError::SymbolNotFound(id),
                    })?)
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
