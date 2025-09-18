use crate::parser::ast::item::Item;
use crate::parser::ast::statement::Statement;
use crate::{ir::typemap::typemap, parser::ast::compunit::CompUnit};
use koopa::ir::builder::{BasicBlockBuilder, LocalInstBuilder, ValueBuilder};
use koopa::ir::{FunctionData, Program, Value};

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
                let func_data = FunctionData::new(
                    format!("@{}", f.ident.0.clone()),
                    vec![],
                    typemap(&f.return_type),
                );
                let func = prog.new_func(func_data);
                let func_data = prog.func_mut(func);
                let dfg = func_data.dfg_mut();
                let entry_bb = dfg.new_bb().basic_block(Some("%entry".to_string()));
                func_data
                    .layout_mut()
                    .bbs_mut()
                    .push_key_back(entry_bb)
                    .unwrap();
                for stmt in &f.body {
                    let stmt = self.build_stmt(stmt, func_data);
                    func_data
                        .layout_mut()
                        .bb_mut(entry_bb)
                        .insts_mut()
                        .push_key_back(stmt)
                        .unwrap();
                }
            }
        }
    }

    fn build_stmt(&self, stmt: &Statement, func: &mut FunctionData) -> Value {
        match stmt {
            Statement::Return(val) => {
                let ret_val = func.dfg_mut().new_value().integer(*val);
                let ret_stmt = func.dfg_mut().new_value().ret(Some(ret_val));
                ret_stmt
            }
        }
    }
}
