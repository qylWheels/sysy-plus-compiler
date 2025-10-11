use crate::parser::ast::common::*;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SymbolTableError {
    #[error("{0} is not exists in symbol table")]
    SymbolNotFound(Ident),
}

#[derive(Debug, Clone)]
pub struct SymbolTable {
    map: HashMap<Ident, SymbolInfo>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// 往符号表中添加符号。若同名，新的覆盖旧的
    pub fn add_symbol(&mut self, id: Ident, syminfo: SymbolInfo) {
        self.map
            .entry(id)
            .and_modify(|sym| *sym = syminfo.clone())
            .or_insert(syminfo.clone());
    }

    /// 删除符号表中的符号。若不存在，则忽略，不报错
    pub fn remove_symbol(&mut self, id: &Ident) {
        self.map.remove(id);
    }

    pub fn find_symbol(&self, id: &Ident) -> Result<&SymbolInfo, SymbolTableError> {
        self.map
            .get(id)
            .ok_or(SymbolTableError::SymbolNotFound(id.clone()))
    }
}

#[derive(Debug, Error)]
pub enum SymbolInfoError {
    #[error("this is not a constant value")]
    NotConstValue,
}

#[derive(Debug, Clone)]
pub(crate) struct SymbolInfo {
    pub(crate) qualifier: Qualifier,
    pub(crate) ty: Type,
    pub(crate) const_val: Option<i32>,
}

impl SymbolInfo {
    pub fn is_const_val(&self) -> bool {
        match self.const_val {
            Some(_) => true,
            None => false,
        }
    }

    pub fn const_val_of(&self) -> Result<i32, SymbolInfoError> {
        match self.const_val {
            Some(i) => Ok(i),
            None => Err(SymbolInfoError::NotConstValue),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Qualifier {
    Var,
    Const,
}
