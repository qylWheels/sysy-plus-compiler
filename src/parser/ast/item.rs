use super::common::*;
use super::statement::*;

#[derive(Debug, Clone)]
pub enum Item {
    FuncDef(FuncDef),
    GlobalVar(Statement),
}

#[derive(Debug, Clone)]
pub struct FuncDef {
    pub(crate) ident: Identifier,
    pub(crate) fparams: Vec<(Identifier, Type)>,
    pub(crate) return_type: Type,
    pub(crate) body: Vec<Statement>,
}
