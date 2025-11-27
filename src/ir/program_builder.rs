use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::parser::ast::common::ResolveStatus;
use crate::parser::ast::expression::{BinaryOp, Expression, UnaryOp};
use crate::parser::ast::item::Item;
use crate::parser::ast::statement::Statement;
use crate::semantic::symbol_table::{SymbolInfoError, SymbolTable, SymbolTableError};
use crate::{ir::typemap::typemap, parser::ast::compile_unit::CompileUnit};
use koopa::ir::builder::{BasicBlockBuilder, LocalInstBuilder, ValueBuilder};
use koopa::ir::{BasicBlock, FunctionData, Program, Type, Value};
use thiserror::Error;

macro_rules! new_value {
    ($func_data:expr) => {
        new_instr!($func_data)
    };
}

macro_rules! new_instr {
    ($func_data:expr) => {
        $func_data.dfg_mut().new_value()
    };
}

macro_rules! add_instr {
    ($func_data:expr, $bb:expr, $instr:expr) => {
        $func_data
            .layout_mut()
            .bb_mut($bb)
            .insts_mut()
            .push_key_back($instr)
            .unwrap();
    };
}

macro_rules! new_bb {
    ($func_data:expr, $name:expr) => {
        $func_data
            .dfg_mut()
            .new_bb()
            .basic_block(Some($name.to_string()))
    };
}

macro_rules! add_bb {
    ($func_data:expr, $bb:expr) => {
        $func_data
            .layout_mut()
            .bbs_mut()
            .push_key_back($bb)
            .unwrap();
    };
}

#[derive(Debug, Error)]
pub enum ProgramBuilderError {
    #[error("symbol table error: {0}")]
    SymbolTableError(#[from] SymbolTableError),

    #[error("symbol information error: {0}")]
    SymbolInfoError(#[from] SymbolInfoError),

    #[error("symbol-value map error: {0}")]
    SymbolValueMapError(#[from] SymbolValueMapError),
}

pub struct ProgramBuilder {
    ast: CompileUnit,
    // TODO: 废弃symbol_table字段
    // symbol_table: SymbolTable,
    symbol_value_map: SymbolValueMap,
}

impl ProgramBuilder {
    pub fn new(ast: CompileUnit) -> Self {
        Self {
            ast,
            symbol_value_map: SymbolValueMap::new(),
        }
    }

    pub fn build_compunit(&mut self) -> Result<Program, ProgramBuilderError> {
        let mut prog = Program::new();
        let items = &self.ast.items.clone();
        for item in items {
            self.build_item(&mut prog, item)?;
        }
        Ok(prog)
    }

    fn build_item(&mut self, prog: &mut Program, item: &Item) -> Result<(), ProgramBuilderError> {
        match item {
            Item::FuncDef(f) => {
                // 进入子作用域
                let new_scope = self.symbol_value_map.enter_scope();
                self.symbol_value_map = new_scope;

                // 创建函数框架
                let func = prog.new_func(FunctionData::new(
                    format!("@{}", f.ident.name),
                    vec![],
                    typemap(&f.return_type),
                ));
                let func_data = prog.func_mut(func);
                let entry_bb = new_bb!(func_data, "%entry");
                add_bb!(func_data, entry_bb);

                // 生成函数中的语句
                for stmt in &f.body {
                    self.build_stmt(stmt, func_data, entry_bb)?;
                }

                // 返回父作用域
                let old_scope = self.symbol_value_map.exit_scope()?;
                self.symbol_value_map = old_scope;

                Ok(())
            }
        }
    }

    fn build_stmt(
        &mut self,
        stmt: &Statement,
        func_data: &mut FunctionData,
        bb: BasicBlock,
    ) -> Result<(), ProgramBuilderError> {
        match stmt {
            Statement::Return(expr) => {
                let ret_val = self.build_expr(expr, func_data, bb)?;
                let ret_stmt = func_data.dfg_mut().new_value().ret(Some(ret_val));
                add_instr!(func_data, bb, ret_stmt);
                Ok(())
            }
            Statement::ConstDecl(_) => Ok(()),
            Statement::VarDecl(v) => {
                for (_, id, expr_opt) in v {
                    let alloc = new_value!(func_data).alloc(Type::get_i32());
                    add_instr!(func_data, bb, alloc);
                    self.symbol_value_map.add_symbol(id.name.clone(), alloc);
                    if let Some(expr) = expr_opt {
                        let val = self.build_expr(expr, func_data, bb)?;
                        let store = new_value!(func_data).store(val, alloc);
                        add_instr!(func_data, bb, store);
                    }
                }
                Ok(())
            }
            Statement::Assign(lval, expr) => {
                let lval = self.symbol_value_map.find_symbol(&lval.name);
                // FIXME: 找不到符号的问题出在这
                // dbg!(&expr);
                let value = self.build_expr(expr, func_data, bb)?;
                let instr = new_instr!(func_data).store(value, lval);
                add_instr!(func_data, bb, instr);
                Ok(())
            }
            Statement::Expression(expr_opt) => match expr_opt {
                Some(expr) => self.build_expr(expr, func_data, bb).map(|_| ()),
                None => Ok(()),
            },
            Statement::Block(b) => {
                // 进入子作用域
                let new_scope = self.symbol_value_map.enter_scope();
                self.symbol_value_map = new_scope;

                // 生成块中的语句
                for stmt in b {
                    self.build_stmt(*&stmt, func_data, bb)?;
                }

                // 返回父作用域
                let parent_scope = self.symbol_value_map.exit_scope()?;
                self.symbol_value_map = parent_scope;

                Ok(())
            }
        }
    }

    // FIXME: 找不到符号的问题出在这
    fn build_expr(
        &self,
        expr: &Expression,
        func_data: &mut FunctionData,
        bb: BasicBlock,
    ) -> Result<Value, ProgramBuilderError> {
        match expr {
            Expression::IntLit(i) => Ok(func_data.dfg_mut().new_value().integer(*i)),
            Expression::Ident(id) => {
                // dbg!(&id.name);
                // dbg!(&self.symbol_table);
                // println!("=====================================");
                let syminfo = match &*id.resolve_status.borrow() {
                    ResolveStatus::Resolved(syminfo) => syminfo.clone(),
                    ResolveStatus::Unresolved => unreachable!(),
                };
                match syminfo.is_const_val() {
                    true => {
                        let val = syminfo.const_val_of()?;
                        Ok(new_value!(func_data).integer(val))
                    }
                    false => {
                        let value = self.symbol_value_map.find_symbol(&id.name);
                        let instr = new_instr!(func_data).load(value);
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                }
            }
            Expression::Unary(op, expr) => {
                let value = self.build_expr(expr, func_data, bb)?;
                let zero = func_data.dfg_mut().new_value().integer(0);
                match op {
                    UnaryOp::Plus => Ok(value),
                    UnaryOp::Minus => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Sub,
                            zero,
                            value,
                        );
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    UnaryOp::LogicalNot => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Eq,
                            zero,
                            value,
                        );
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                }
            }
            Expression::Binary(e1, op, e2) => {
                let v1 = self.build_expr(e1, func_data, bb)?;
                let v2 = self.build_expr(e2, func_data, bb)?;
                let zero = new_instr!(func_data).integer(0);
                match op {
                    BinaryOp::Add => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Add,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::Sub => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Sub,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::Mul => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Mul,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::Div => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Div,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::Rem => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Mod,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::Less => {
                        let instr =
                            func_data
                                .dfg_mut()
                                .new_value()
                                .binary(koopa::ir::BinaryOp::Lt, v1, v2);
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::Le => {
                        let instr =
                            func_data
                                .dfg_mut()
                                .new_value()
                                .binary(koopa::ir::BinaryOp::Le, v1, v2);
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::Eq => {
                        let instr =
                            func_data
                                .dfg_mut()
                                .new_value()
                                .binary(koopa::ir::BinaryOp::Eq, v1, v2);
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::Ge => {
                        let instr = new_instr!(func_data).binary(koopa::ir::BinaryOp::Ge, v1, v2);
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::Greater => {
                        let instr = new_instr!(func_data).binary(koopa::ir::BinaryOp::Gt, v1, v2);
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::NotEq => {
                        let instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v1, v2);
                        add_instr!(func_data, bb, instr);
                        Ok(instr)
                    }
                    BinaryOp::LogicalAnd => {
                        let v1_instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v1, zero);
                        add_instr!(func_data, bb, v1_instr);

                        let v2_instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v2, zero);
                        add_instr!(func_data, bb, v2_instr);

                        let result = new_instr!(func_data).binary(
                            koopa::ir::BinaryOp::And,
                            v1_instr,
                            v2_instr,
                        );
                        add_instr!(func_data, bb, result);

                        Ok(result)
                    }
                    BinaryOp::LogicalOr => {
                        let v1_instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v1, zero);
                        add_instr!(func_data, bb, v1_instr);

                        let v2_instr =
                            new_instr!(func_data).binary(koopa::ir::BinaryOp::NotEq, v2, zero);
                        add_instr!(func_data, bb, v2_instr);

                        let result = new_instr!(func_data).binary(
                            koopa::ir::BinaryOp::Or,
                            v1_instr,
                            v2_instr,
                        );
                        add_instr!(func_data, bb, result);

                        Ok(result)
                    }
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum SymbolValueMapError {
    #[error("no parent scope")]
    NoParentScope,
}

#[derive(Debug, Clone)]
struct SymbolValueMap {
    parent: Option<Rc<RefCell<Self>>>,
    map: HashMap<String, Value>,
}

impl SymbolValueMap {
    fn new() -> Self {
        Self {
            parent: None,
            map: HashMap::new(),
        }
    }

    /// 往当前层级添加ident-value映射
    fn add_symbol(&mut self, id: String, value: Value) {
        self.map.insert(id, value);
    }

    /// 从当前层级逐级往上查找ident
    fn find_symbol(&self, id: &str) -> Value {
        match self.map.get(id) {
            Some(val) => *val,
            None => match &self.parent {
                Some(parent) => (*parent).borrow().find_symbol(id),
                None => unreachable!(),
            },
        }
    }

    fn enter_scope(&self) -> Self {
        let mut new_scope = Self::new();
        new_scope.parent = Some(Rc::new(RefCell::new(self.clone())));
        new_scope
    }

    fn exit_scope(&self) -> Result<Self, SymbolValueMapError> {
        match self.parent.as_ref() {
            Some(parent) => Ok((*parent).borrow().clone()),
            None => Err(SymbolValueMapError::NoParentScope),
        }
    }
}
