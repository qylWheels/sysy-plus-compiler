use super::common::*;
use super::statement::*;

#[derive(Debug, Clone)]
pub enum Item {
    FuncDef(FuncDef),
}

#[derive(Debug, Clone)]
pub struct FuncDef {
    pub(crate) return_type: Type,
    pub(crate) ident: Identifier,
    pub(crate) body: Vec<Statement>,
}
