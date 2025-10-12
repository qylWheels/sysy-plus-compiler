use std::collections::HashMap;

use crate::parser::ast::common::Ident;
use crate::parser::ast::expression::{BinaryOp, Expression, UnaryOp};
use crate::parser::ast::item::Item;
use crate::parser::ast::statement::Statement;
use crate::semantic::symbol_table::{SymbolInfoError, SymbolTable, SymbolTableError};
use crate::{ir::typemap::typemap, parser::ast::compunit::CompUnit};
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
}

pub struct ProgramBuilder<'a> {
    ast: CompUnit,
    symbol_table: &'a SymbolTable,
    symbol_value_map: SymbolValueMap,
}

impl<'a> ProgramBuilder<'a> {
    pub fn new(ast: CompUnit, symbol_table: &'a SymbolTable) -> Self {
        Self {
            ast,
            symbol_table,
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
                let func = prog.new_func(FunctionData::new(
                    format!("@{}", f.ident.0.clone()),
                    vec![],
                    typemap(&f.return_type),
                ));
                let func_data = prog.func_mut(func);
                let entry_bb = new_bb!(func_data, "%entry");
                add_bb!(func_data, entry_bb);
                for stmt in &f.body {
                    self.build_stmt(stmt, func_data, entry_bb)?;
                }
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
                    self.symbol_value_map.add_symbol(id.clone(), alloc);
                    if let Some(expr) = expr_opt {
                        let val = self.build_expr(expr, func_data, bb)?;
                        let store = new_value!(func_data).store(val, alloc);
                        add_instr!(func_data, bb, store);
                    }
                }
                Ok(())
            }
            Statement::Assign(lval, expr) => {
                let lval = self.symbol_value_map.find_symbol(lval);
                let value = self.build_expr(expr, func_data, bb)?;
                let instr = new_instr!(func_data).store(value, lval);
                add_instr!(func_data, bb, instr);
                Ok(())
            }
        }
    }

    fn build_expr(
        &self,
        expr: &Expression,
        func_data: &mut FunctionData,
        bb: BasicBlock,
    ) -> Result<Value, ProgramBuilderError> {
        match expr {
            Expression::IntLit(i) => Ok(func_data.dfg_mut().new_value().integer(*i)),
            Expression::Ident(id) => {
                let symbol = self.symbol_table.find_symbol(id)?;
                match symbol.is_const_val() {
                    true => {
                        let val = symbol.const_val_of()?;
                        Ok(new_value!(func_data).integer(val))
                    }
                    false => {
                        let value = self.symbol_value_map.find_symbol(id);
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

#[derive(Debug, Clone)]
struct SymbolValueMap {
    map: HashMap<Ident, Value>,
}

impl SymbolValueMap {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn add_symbol(&mut self, id: Ident, value: Value) {
        self.map.insert(id, value);
    }

    fn find_symbol(&self, id: &Ident) -> Value {
        *self.map.get(id).unwrap()
    }
}
