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

    /// 条件, then分支, else分支
    If(Expression, Box<Statement>, Option<Box<Statement>>),

    /// 条件，语句
    While(Expression, Box<Statement>),

    Break,
    Continue,
}
