#[derive(Debug, Clone)]
pub enum Expression {
    IntLit(i32),
    Unary(UnaryOp, Box<Expression>),
    Binary(Box<Expression>, BinaryOp, Box<Expression>),
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Plus,
    Minus,
    LogicalNot,
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    // 算术运算符
    Add,
    Sub,
    Mul,
    Div,
    Rem,

    // 逻辑运算符
    Less,
    Le,
    Eq,
    Ge,
    Greater,
    NotEq,
}
