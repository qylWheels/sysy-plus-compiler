use super::common::*;
use super::expression::*;

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Statement {
    Return(Expression),
    ConstDecl(Vec<(Type, Identifier, Expression)>),
    VarDecl(Vec<(Type, Identifier, Option<Expression>)>),
    Assign(Identifier, Expression),
    Expression(Option<Expression>),
    Block(Vec<Box<Statement>>),
}
