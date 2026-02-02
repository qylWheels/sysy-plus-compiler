use std::{cell::RefCell, fmt::Display};

use uuid::Uuid;

use crate::semantic::symbol_table::SymbolInfo;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Void,
    Simple(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    /// 标识符的名称
    pub(crate) name: String,

    /// 语义检查后其对应的SymbolInfo
    pub(crate) resolve_status: RefCell<ResolveStatus>,

    /// uuid，因为可能存在两个相同名称且SymbolInfo相等的Identifier
    pub(crate) uuid: Uuid,
}

impl Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveStatus {
    Unresolved,
    Resolved(SymbolInfo),
}
