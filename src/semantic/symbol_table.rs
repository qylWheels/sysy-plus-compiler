use crate::parser::ast::common::*;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SymbolTableError {
    #[error("{0} is not exists in symbol table")]
    SymbolNotFound(String),

    #[error("{0} is redefined")]
    SymbolRedefined(String),

    #[error("this scope has no parent scope")]
    NoParentScope,
}

#[derive(Debug, Clone)]
pub struct SymbolTable {
    parent: Option<Rc<RefCell<SymbolTable>>>,
    map: HashMap<String, SymbolInfo>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            parent: None,
            map: HashMap::new(),
        }
    }

    pub fn set_parent(&mut self, parent: &SymbolTable) {
        self.parent = Some(Rc::new(RefCell::new(parent.clone())));
    }

    /// 往符号表当前层级中添加符号。若同名，报错
    pub fn add_symbol(
        &mut self,
        id: String,
        syminfo: SymbolInfo,
    ) -> Result<(), SymbolTableError> {
        match self.map.get(&id) {
            Some(_) => Err(SymbolTableError::SymbolRedefined(id.clone())),
            None => {
                self.map.insert(id, syminfo);
                Ok(())
            }
        }
    }

    /// 删除符号表当前层级中的符号。若不存在，则忽略，不报错
    pub fn remove_symbol(&mut self, id: &String) {
        self.map.remove(id);
    }

    /// 从当前层级到顶级，依次查找符号表中的符号
    pub fn find_symbol(&self, id: &String) -> Result<SymbolInfo, SymbolTableError> {
        let result = self.map.get(id);
        match result {
            Some(info) => Ok(info.clone()),
            None => {
                if self.is_top_scope() {
                    Err(SymbolTableError::SymbolNotFound((*id).clone()))
                } else {
                    self.parent
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .borrow()
                        .find_symbol(id)
                }
            }
        }
    }

    /// 进入新的子作用域
    pub fn enter_scope(&self) -> SymbolTable {
        let mut child = Self::new();
        child.set_parent(&self);
        child
    }

    /// 判断自身是否已经是顶层作用域（即没有父作用域）
    pub fn is_top_scope(&self) -> bool {
        match self.parent {
            Some(_) => false,
            None => true,
        }
    }

    /// 退出作用域，返回到父作用域
    /// invariant: 自身必须有父作用域，否则返回错误
    pub fn exit_scope(&self) -> Result<Rc<RefCell<SymbolTable>>, SymbolTableError> {
        match self.parent.as_ref() {
            Some(parent) => Ok(Rc::clone(parent)),
            None => Err(SymbolTableError::NoParentScope),
        }
    }
}

#[derive(Debug, Error)]
pub enum SymbolInfoError {
    #[error("this is not a constant value")]
    NotConstValue,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub(crate) qualifier: Qualifier,
    pub(crate) ty: Type,
    pub(crate) const_val: Option<i32>,
}

impl SymbolInfo {
    pub fn is_const_val(&self) -> bool {
        match self.qualifier {
            Qualifier::Const => true,
            Qualifier::Var => false,
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
