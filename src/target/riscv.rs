use core::panic;
use std::io;

use koopa::ir::{entities::ValueData, *};

const INDENT_SIZE: usize = 2;

#[derive(Clone)]
pub struct Context<'a> {
    pub func: Option<&'a FunctionData>,
    pub indent: usize,
}

pub trait GenerateRiscv {
    fn generate(&self, dest: &mut impl io::Write, ctx: Context);
}

impl GenerateRiscv for Program {
    fn generate(&self, dest: &mut impl io::Write, ctx: Context) {
        writeln!(dest, "{}.text", " ".repeat(ctx.indent + INDENT_SIZE)).unwrap();
        for func in self.func_layout() {
            let func_data = self.func(*func);
            func_data.generate(dest, ctx.clone());
        }
    }
}

impl GenerateRiscv for FunctionData {
    fn generate(&self, dest: &mut impl io::Write, ctx: Context) {
        // 生成globl声明和符号
        writeln!(
            dest,
            "{}.globl {}",
            " ".repeat(ctx.indent + INDENT_SIZE),
            self.name()[1..].to_string()
        )
        .unwrap();
        writeln!(
            dest,
            "{}{}:",
            " ".repeat(ctx.indent),
            self.name()[1..].to_string()
        )
        .unwrap();

        for (_, node) in self.layout().bbs() {
            for &inst in node.insts().keys() {
                let value_data = self.dfg().value(inst);
                value_data.generate(
                    dest,
                    Context {
                        func: Some(self),
                        indent: ctx.indent + INDENT_SIZE,
                    },
                );
            }
        }
    }
}

impl GenerateRiscv for ValueData {
    fn generate(&self, dest: &mut impl io::Write, ctx: Context) {
        match self.kind() {
            ValueKind::Integer(_) => panic!("Single integer instruction not supported"),
            ValueKind::Return(r) => {
                let ret = match ctx.func {
                    Some(f) => {
                        let retval = f.dfg().value(r.value().unwrap());
                        match retval.kind() {
                            ValueKind::Integer(i) => i.value(),
                            _ => unimplemented!(),
                        }
                    }
                    None => panic!("Return must be in a function"),
                };
                writeln!(dest, "{}li a0, {}", " ".repeat(ctx.indent), ret).unwrap();
                writeln!(dest, "{}ret", " ".repeat(ctx.indent)).unwrap();
            }
            _ => unimplemented!(),
        }
    }
}
