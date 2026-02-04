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

struct SymbolTableInner {
    parent: Option<Rc<RefCell<SymbolTableInner>>>,
    map: HashMap<String, SymbolInfo>,
}

#[derive(Debug, Clone)]
pub(crate) struct SymbolTable {
    inner: Rc<RefCell<SymbolTableInner>>,
}

impl SymbolTable {
    pub(crate) fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(SymbolTableInner {
                parent: None,
                map: HashMap::new(),
            })),
        }
    }

    pub(crate) fn set_parent(&self, parent: &SymbolTable) {
        self.inner.borrow_mut().parent = Some(Rc::clone(&parent.inner));
    }

    /// 往符号表当前层级中添加符号。若同名，报错
    pub(crate) fn add_symbol(
        &self,
        id: String,
        syminfo: SymbolInfo,
    ) -> Result<(), SymbolTableError> {
        let mut inner = self.inner.borrow_mut();
        let result = inner.map.get(&id);
        match result {
            Some(_) => Err(SymbolTableError::SymbolRedefined(id.clone())),
            None => {
                inner.map.insert(id, syminfo);
                Ok(())
            }
        }
    }

    /// 删除符号表当前层级中的符号。若不存在，则忽略，不报错
    pub(crate) fn remove_symbol(&self, id: &String) {
        self.inner.borrow_mut().map.remove(id);
    }

    /// 从当前层级到顶级，依次查找符号表中的符号
    /// TODO: 这样的作用域管理模式不好，违反单一职责原则。应该由symtable自己管理作用域。
    /// 考虑提供一个with_scope()，其接收一个闭包，使得闭包内的操作在子作用域中完成。
    /// 完成后自动返回父作用域。
    pub(crate) fn find_symbol(&self, id: &String) -> Result<SymbolInfo, SymbolTableError> {
        match self.inner.borrow().map.get(id) {
            Some(info) => Ok(info.clone()),
            None => {
                if self.is_top_scope() {
                    Err(SymbolTableError::SymbolNotFound((*id).clone()))
                } else {
                    let symtable = SymbolTable {
                        inner: Rc::clone(self.inner.borrow().parent.as_ref().unwrap()),
                    };
                    symtable.find_symbol(id)
                }
            }
        }
    }

    /// 进入新的子作用域
    #[deprecated(note = "Use \"with_scope()\" instead")]
    pub(crate) fn enter_scope(&self) -> SymbolTable {
        let child = Self::new();
        child.set_parent(&self);
        child
    }

    /// 判断自身是否已经是顶层作用域（即没有父作用域）
    pub(crate) fn is_top_scope(&self) -> bool {
        match self.inner.borrow().parent {
            Some(_) => false,
            None => true,
        }
    }

    /// 退出作用域，返回到父作用域
    /// invariant: 自身必须有父作用域，否则返回错误
    #[deprecated(note = "Use \"with_scope()\" instead")]
    pub(crate) fn exit_scope(&self) -> Result<SymbolTable, SymbolTableError> {
        match self.inner.borrow().parent.as_ref() {
            Some(parent) => Ok(SymbolTable {
                inner: Rc::clone(parent),
            }),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    pub(crate) qualifier: Qualifier,
    pub(crate) ty: Type,
    pub(crate) const_val: Option<i32>,
}

impl SymbolInfo {
    pub(crate) fn is_const_val(&self) -> bool {
        match self.qualifier {
            Qualifier::Const => true,
            Qualifier::Var => false,
        }
    }

    pub(crate) fn const_val_of(&self) -> Result<i32, SymbolInfoError> {
        match self.const_val {
            Some(i) => Ok(i),
            None => Err(SymbolInfoError::NotConstValue),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Qualifier {
    Var,
    Const,
}
