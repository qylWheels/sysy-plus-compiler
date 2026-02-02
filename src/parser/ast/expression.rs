use super::common::Identifier;

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Expression {
    IntLit(i32),
    Ident(Identifier),
    Unary(UnaryOp, Box<Expression>),
    Binary(Box<Expression>, BinaryOp, Box<Expression>),
    Call(Identifier, Vec<Expression>),
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Plus,
    Minus,
    LogicalNot,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    // 算术运算符
    Add,
    Sub,
    Mul,
    Div,
    Rem,

    // 比较运算符
    Less,
    Le,
    Eq,
    Ge,
    Greater,
    NotEq,

    // 逻辑运算符
    LogicalAnd,
    LogicalOr,
}
