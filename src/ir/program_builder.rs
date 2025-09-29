use crate::parser::ast::expression::{BinaryOp, Expression, UnaryOp};
use crate::parser::ast::item::Item;
use crate::parser::ast::statement::Statement;
use crate::{ir::typemap::typemap, parser::ast::compunit::CompUnit};
use koopa::ir::builder::{BasicBlockBuilder, LocalInstBuilder, ValueBuilder};
use koopa::ir::{BasicBlock, FunctionData, Program, Value};

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

pub struct ProgramBuilder {
    ast: CompUnit,
}

impl ProgramBuilder {
    pub fn new(ast: CompUnit) -> Self {
        Self { ast }
    }

    pub fn build_compunit(&self) -> Program {
        let mut prog = Program::new();
        for item in &self.ast.items {
            self.build_item(&mut prog, item);
        }
        prog
    }

    fn build_item(&self, prog: &mut Program, item: &Item) {
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
                    self.build_stmt(stmt, func_data, entry_bb);
                }
            }
        }
    }

    fn build_stmt(&self, stmt: &Statement, func_data: &mut FunctionData, bb: BasicBlock) {
        match stmt {
            Statement::Return(expr) => {
                let ret_val = self.build_expr(expr, func_data, bb);
                let ret_stmt = func_data.dfg_mut().new_value().ret(Some(ret_val));
                add_instr!(func_data, bb, ret_stmt);
            }
        }
    }

    fn build_expr(&self, expr: &Expression, func_data: &mut FunctionData, bb: BasicBlock) -> Value {
        match expr {
            Expression::IntLit(i) => func_data.dfg_mut().new_value().integer(*i),
            Expression::Unary(op, expr) => {
                let value = self.build_expr(expr, func_data, bb);
                let zero = func_data.dfg_mut().new_value().integer(0);
                match op {
                    UnaryOp::Plus => value,
                    UnaryOp::Minus => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Sub,
                            zero,
                            value,
                        );
                        add_instr!(func_data, bb, instr);
                        instr
                    }
                    UnaryOp::LogicalNot => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Eq,
                            zero,
                            value,
                        );
                        add_instr!(func_data, bb, instr);
                        instr
                    }
                }
            }
            Expression::Binary(e1, op, e2) => {
                let v1 = self.build_expr(e1, func_data, bb);
                let v2 = self.build_expr(e2, func_data, bb);
                match op {
                    BinaryOp::Add => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Add,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        instr
                    }
                    BinaryOp::Sub => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Sub,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        instr
                    }
                    BinaryOp::Mul => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Mul,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        instr
                    }
                    BinaryOp::Div => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Div,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        instr
                    }
                    BinaryOp::Rem => {
                        let instr = func_data.dfg_mut().new_value().binary(
                            koopa::ir::BinaryOp::Mod,
                            v1,
                            v2,
                        );
                        add_instr!(func_data, bb, instr);
                        instr
                    }
                    _ => unimplemented!(),
                }
            }
        }
    }
}
