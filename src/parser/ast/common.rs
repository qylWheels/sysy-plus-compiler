use std::{cell::RefCell, fmt::Display, hash::Hash, rc::Rc};

use crate::semantic::symbol_table::SymbolTable;

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Type {
    Simple(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    /// 标识符的名称
    pub(crate) name: String,

    /// 标识符所属的作用域
    pub(crate) scope: RefCell<ResolveStatus>,
}

impl Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Hash for Identifier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

#[derive(Debug, Clone)]
pub enum ResolveStatus {
    Unresolved,
    Resolved(Rc<RefCell<SymbolTable>>),
}

impl PartialEq for ResolveStatus {
    fn eq(&self, other: &Self) -> bool {
        match (&self, other) {
            (Self::Unresolved, Self::Unresolved) => true,
            (Self::Resolved(rc1), Self::Resolved(rc2)) => {
                if Rc::as_ptr(rc1) == Rc::as_ptr(rc2) {
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl Eq for ResolveStatus {}

impl Hash for ResolveStatus {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match &self {
            Self::Unresolved => 0.hash(state),
            Self::Resolved(rc) => (Rc::as_ptr(rc) as usize).hash(state),
        }
    }
}
