use std::rc::Rc;

use crate::parser::ast::common::*;
use crate::parser::ast::expression::*;

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Statement {
    Return(Expression),
    ConstDecl(Vec<(Type, Rc<Identifier>, Expression)>),
    VarDecl(Vec<(Type, Rc<Identifier>, Option<Expression>)>),
    Assign(Rc<Identifier>, Expression),
    Expression(Option<Expression>),
    Block(Vec<Box<Statement>>)
}
