use std::collections::*;
use std::io;

use koopa::ir::*;

use crate::target::riscv::reg_alloc::{Allocation, RegAllocator};

const INDENT_SIZE: usize = 2;
#[derive(Clone)]
pub struct Context<'a> {
    pub func: Option<&'a FunctionData>,
    pub indent: usize,
    pub reg_alloc_result: Option<&'a HashMap<Value, Allocation>>,
}

/// 有些ir指令不会直接对应一条riscv指令（如Integer），这时通过该枚举将其返回，让其组成其它指令的部分
#[derive(Debug, Clone)]
pub enum ResultValue {
    None,
    Integer(i32),
    KoopaRegister(Value),
}

pub trait GenerateRiscv {
    fn generate(&self, dest: &mut impl io::Write, ctx: Context) -> ResultValue;
}

impl GenerateRiscv for Program {
    fn generate(&self, dest: &mut impl io::Write, ctx: Context) -> ResultValue {
        writeln!(dest, "{}.text", " ".repeat(ctx.indent + INDENT_SIZE)).unwrap();
        for func in self.func_layout() {
            let func_data = self.func(*func);
            func_data.generate(dest, ctx.clone());
        }
        ResultValue::None
    }
}

impl GenerateRiscv for FunctionData {
    fn generate(&self, dest: &mut impl io::Write, ctx: Context) -> ResultValue {
        // 生成globl声明
        writeln!(
            dest,
            "{}.globl {}",
            " ".repeat(ctx.indent + INDENT_SIZE),
            self.name()[1..].to_string()
        )
        .unwrap();

        // 生成函数名
        writeln!(
            dest,
            "{}{}:",
            " ".repeat(ctx.indent),
            self.name()[1..].to_string()
        )
        .unwrap();

        // 寄存器分配
        let mut reg_allocator = RegAllocator::new();
        let alloc_result = reg_allocator.allocate(self);

        // 生成函数体代码
        for (_, node) in self.layout().bbs() {
            for &inst in node.insts().keys() {
                inst.generate(
                    dest,
                    Context {
                        func: Some(self),
                        indent: ctx.indent + INDENT_SIZE,
                        reg_alloc_result: Some(alloc_result),
                    },
                );
            }
        }

        ResultValue::None
    }
}

macro_rules! get_valuedata {
    ($v:expr, $func_data:expr) => {
        $func_data.dfg().value($v)
    };
}

impl GenerateRiscv for Value {
    fn generate(&self, dest: &mut impl io::Write, ctx: Context) -> ResultValue {
        let value_data = ctx.func.unwrap().dfg().value(*self);
        match value_data.kind() {
            ValueKind::Integer(i) => ResultValue::Integer(i.value()),
            ValueKind::Return(r) => {
                let ret = r.value();
                match ret {
                    Some(v) => {
                        // let result = v.generate(dest, Context { ..ctx });
                        if is_koopa_reg(v, ctx.func.unwrap()) {
                            let reg = ctx.reg_alloc_result.unwrap().get(&v).unwrap();
                            let reg_str = match reg {
                                Allocation::Register(r) => r.to_string(),
                                _ => unimplemented!(),
                            };
                            writeln!(dest, "{}mv a0, {}", " ".repeat(ctx.indent), reg_str).unwrap();
                        } else {
                            let result = match ctx.func.unwrap().dfg().value(v).kind() {
                                ValueKind::Integer(i) => i.value().to_string(),
                                _ => unimplemented!(),
                            };
                            writeln!(dest, "{}li a0, {}", " ".repeat(ctx.indent), result).unwrap();
                        }
                    }
                    None => (),
                }
                writeln!(dest, "{}ret", " ".repeat(ctx.indent)).unwrap();

                ResultValue::None
            }
            ValueKind::Binary(b) => {
                let (lhs, op, rhs) = (b.lhs(), b.op(), b.rhs());
                let lhs_valuedata = get_valuedata!(lhs, ctx.func.unwrap());
                let rhs_valuedata = get_valuedata!(rhs, ctx.func.unwrap());
                let lhs_is_koopa_reg = is_koopa_reg(lhs, ctx.func.unwrap());
                let rhs_is_koopa_reg = is_koopa_reg(rhs, ctx.func.unwrap());

                // lhs和rhs分配到的的riscv寄存器
                let lhs_riscv_reg_str = match ctx.reg_alloc_result.unwrap().get(&lhs).unwrap() {
                    Allocation::Register(reg) => reg.to_string(),
                    _ => unimplemented!(),
                };
                let rhs_riscv_reg_str = match ctx.reg_alloc_result.unwrap().get(&rhs).unwrap() {
                    Allocation::Register(reg) => reg.to_string(),
                    _ => unimplemented!(),
                };

                // 当lhs和rhs为integer时用这些
                let lhs_int = match lhs_valuedata.kind() {
                    ValueKind::Integer(i) => Some(i.value()),
                    _ => None,
                };
                let rhs_int = match rhs_valuedata.kind() {
                    ValueKind::Integer(i) => Some(i.value()),
                    _ => None,
                };

                // 自己一定是个koopa reg
                let self_riscv_reg = match ctx.reg_alloc_result.unwrap().get(&self).unwrap() {
                    Allocation::Register(r) => r,
                    _ => unimplemented!(),
                };
                let self_str = self_riscv_reg.to_string();

                match (lhs_is_koopa_reg, op, rhs_is_koopa_reg) {
                    // 加
                    (false, BinaryOp::Add, false) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            (lhs_int.unwrap() + rhs_int.unwrap()).to_string()
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Add, false) => {
                        writeln!(
                            dest,
                            "{}addi {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_int.unwrap().to_string()
                        )
                        .unwrap();
                    }
                    (false, BinaryOp::Add, true) => {
                        writeln!(
                            dest,
                            "{}addi {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            rhs_riscv_reg_str,
                            lhs_int.unwrap().to_string(),
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Add, true) => {
                        writeln!(
                            dest,
                            "{}add {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str
                        )
                        .unwrap();
                    }

                    // 减
                    (false, BinaryOp::Sub, false) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            (lhs_int.unwrap() - rhs_int.unwrap()).to_string(),
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Sub, false) => {
                        writeln!(
                            dest,
                            "{}addi {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            (-rhs_int.unwrap()).to_string(),
                        )
                        .unwrap();
                    }
                    (false, BinaryOp::Sub, true) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            lhs_riscv_reg_str,
                            lhs_int.unwrap().to_string(),
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}sub {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str,
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Sub, true) => {
                        writeln!(
                            dest,
                            "{}sub {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str
                        )
                        .unwrap();
                    }

                    // 乘
                    (false, BinaryOp::Mul, false) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            (lhs_int.unwrap() * rhs_int.unwrap()).to_string(),
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Mul, false) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            rhs_riscv_reg_str,
                            rhs_int.unwrap().to_string(),
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}mul {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str,
                        )
                        .unwrap();
                    }
                    (false, BinaryOp::Mul, true) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            lhs_riscv_reg_str,
                            lhs_int.unwrap(),
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}mul {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str,
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Mul, true) => {
                        writeln!(
                            dest,
                            "{}mul {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str
                        )
                        .unwrap();
                    }

                    // 除
                    (false, BinaryOp::Div, false) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            (lhs_int.unwrap() / rhs_int.unwrap()).to_string(),
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Div, false) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            rhs_riscv_reg_str,
                            rhs_int.unwrap().to_string(),
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}div {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str,
                        )
                        .unwrap();
                    }
                    (false, BinaryOp::Div, true) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            lhs_riscv_reg_str,
                            lhs_int.unwrap().to_string(),
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}div {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str,
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Div, true) => {
                        writeln!(
                            dest,
                            "{}div {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_riscv_reg.to_string(),
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str
                        )
                        .unwrap();
                    }

                    // 取余
                    (false, BinaryOp::Mod, false) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            lhs_int.unwrap() % rhs_int.unwrap(),
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Mod, false) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            rhs_riscv_reg_str,
                            rhs_int.unwrap(),
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}rem {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str
                        )
                        .unwrap();
                    }
                    (false, BinaryOp::Mod, true) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            lhs_riscv_reg_str,
                            lhs_int.unwrap(),
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}rem {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Mod, true) => {
                        writeln!(
                            dest,
                            "{}rem {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str,
                        )
                        .unwrap();
                    }

                    // 相等
                    (false, BinaryOp::Eq, false) => {
                        writeln!(
                            dest,
                            "{}li {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            i32::from(lhs_riscv_reg_str == rhs_riscv_reg_str),
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Eq, false) => {
                        writeln!(
                            dest,
                            "{}xori {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            lhs_riscv_reg_str,
                            rhs_int.unwrap(),
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}seqz {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            self_str,
                        )
                        .unwrap();
                    }
                    (false, BinaryOp::Eq, true) => {
                        writeln!(
                            dest,
                            "{}xori {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            rhs_riscv_reg_str,
                            lhs_int.unwrap(),
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}seqz {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            self_str,
                        )
                        .unwrap();
                    }
                    (true, BinaryOp::Eq, true) => {
                        writeln!(
                            dest,
                            "{}xor {}, {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            lhs_riscv_reg_str,
                            rhs_riscv_reg_str,
                        )
                        .unwrap();
                        writeln!(
                            dest,
                            "{}seqz {}, {}",
                            " ".repeat(ctx.indent),
                            self_str,
                            self_str,
                        )
                        .unwrap();
                    }

                    _ => unimplemented!(),
                };

                ResultValue::KoopaRegister(*self)
            }
            _ => unimplemented!(),
        }
    }
}

fn is_koopa_reg(value: Value, func_data: &FunctionData) -> bool {
    let value_data = func_data.dfg().value(value);
    let kind = value_data.kind();
    match kind {
        ValueKind::Integer(_) | ValueKind::Return(_) => false,
        _ => true,
    }
}
