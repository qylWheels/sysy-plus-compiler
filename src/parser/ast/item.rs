use super::common::*;
use super::statement::*;

#[derive(Debug, Clone)]
pub enum Item {
    FuncDef(FuncDef),
}

#[derive(Debug, Clone)]
pub struct FuncDef {
    pub return_type: Type,
    pub ident: Ident,
    pub body: Vec<Statement>,
}
