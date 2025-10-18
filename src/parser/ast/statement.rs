use crate::parser::ast::common::*;
use crate::parser::ast::expression::*;

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Statement {
    Return(Expression),
    ConstDecl(Vec<(Type, Ident, Expression)>),
    VarDecl(Vec<(Type, Ident, Option<Expression>)>),
    Assign(Ident, Expression),
    Expression(Option<Expression>),
    Block(Vec<Box<Statement>>)
}
