use crate::parser::ast::expression::*;

#[derive(Debug, Clone)]
pub enum Statement {
    Return(Expression),
}
