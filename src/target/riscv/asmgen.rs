use std::io;

use koopa::ir::*;

use crate::target::riscv::reg_alloc::AllocResult;
use crate::target::riscv::reg_alloc::{Allocation, RegAllocator};

const INDENT_SIZE: usize = 2;
#[derive(Clone)]
pub struct Context<'a> {
    pub func: Option<&'a FunctionData>,
    pub indent: usize,
    pub alloc_result: Option<AllocResult<'a>>,
}

/// 有些ir指令不会直接对应一条riscv指令（如Integer），这时通过该枚举将其返回，让其组成其它指令的部分
#[derive(Debug, Clone)]
pub enum ResultValue {
    None,
    Integer(i32),
    KoopaRegister(Value),
}

/// 用于简化两个操作数的is_koopa_reg值为一真一假时的代码
// macro_rules! handle_true_false {
//     ($case:expr, $dest:expr, $op:expr, $ctx:expr, $self_reg:expr,
//         $lhs_reg:expr, $lhs_int:expr,
//         $rhs_reg:expr, $rhs_int:expr
//     ) => {
//         (true, $case, false) => {
//             writeln!(
//                 $dest,
//                 "{}li {}, {}",
//                 " ".repeat(ctx.indent),
//                 rhs_reg.to_string(),
//                 rhs_int.unwrap(),
//             )
//             .unwrap();
//             writeln!(
//                 $dest,
//                 "{}{} {}, {}, {}",
//                 " ".repeat(ctx.indent),
//                 $op,
//                 $self_reg.to_string(),
//                 $lhs_reg.to_string(),
//                 $rhs_reg.to_string(),
//             )
//             .unwrap();
//         }

//         (false, $case, true) => {
//             writeln!(
//                 $dest,
//                 "{}li {}, {}",
//                 " ".repeat(ctx.indent),
//                 lhs_reg.to_string(),
//                 lhs_int.unwrap(),
//             )
//             .unwrap();
//             writeln!(
//                 $dest,
//                 "{}{} {}, {}, {}",
//                 " ".repeat(ctx.indent),
//                 $op,
//                 $self_reg.to_string(),
//                 $lhs_reg.to_string(),
//                 $rhs_reg.to_string(),
//             )
//             .unwrap();
//         }
//     };
// }

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

        // 对函数中的局部变量进行寄存器分配
        let mut reg_allocator = RegAllocator::new();
        let alloc_result = reg_allocator.allocate(self);

        // 生成prologue
        writeln!(
            dest,
            "{}addi sp, sp, {}",
            " ".repeat(ctx.indent + 2),
            alloc_result.stack_size
        )
        .unwrap();

        // 生成函数体代码
        for (_, node) in self.layout().bbs() {
            for &inst in node.insts().keys() {
                inst.generate(
                    dest,
                    Context {
                        func: Some(self),
                        indent: ctx.indent + INDENT_SIZE,
                        alloc_result: Some(alloc_result.clone()),
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

macro_rules! get_allocation {
    ($v:expr, $ctx:expr) => {
        $ctx.alloc_result.as_ref().unwrap().allocs.get($v).unwrap()
    };
}

impl GenerateRiscv for Value {
    fn generate(&self, dest: &mut impl io::Write, ctx: Context) -> ResultValue {
        let value_data = ctx.func.unwrap().dfg().value(*self);
        match value_data.kind() {
            ValueKind::Integer(i) => ResultValue::Integer(i.value()),
            ValueKind::Return(r) => {
                let ret = r.value();
                let alloc_result = ctx.alloc_result.as_ref().unwrap().clone();
                match ret {
                    Some(v) => {
                        // let result = v.generate(dest, Context { ..ctx });
                        if is_koopa_reg(v, ctx.func.unwrap()) {
                            // 如果是koopa寄存器类型
                            let alloc = alloc_result.allocs.get(&v).unwrap();
                            match alloc {
                                Allocation::Register(r) => writeln!(
                                    dest,
                                    "{}mv a0, {}",
                                    " ".repeat(ctx.indent),
                                    r.to_string()
                                )
                                .unwrap(),
                                Allocation::Spilled(off) => {
                                    writeln!(dest, "{}lw a0, {}(sp)", " ".repeat(ctx.indent), off)
                                        .unwrap();
                                }
                            };
                        } else {
                            // 如果是立即数
                            let result = match ctx.func.unwrap().dfg().value(v).kind() {
                                ValueKind::Integer(i) => i.value(),
                                _ => unimplemented!(),
                            };
                            writeln!(dest, "{}li a0, {}", " ".repeat(ctx.indent), result).unwrap();
                        }
                    }
                    None => (),
                }

                // 生成函数epilogue
                writeln!(
                    dest,
                    "{}addi sp, sp, -{}",
                    " ".repeat(ctx.indent),
                    alloc_result.stack_size
                )
                .unwrap();

                // 生成ret指令
                writeln!(dest, "{}ret", " ".repeat(ctx.indent)).unwrap();

                ResultValue::None
            }
            ValueKind::Binary(b) => {
                let (lhs, op, rhs) = (b.lhs(), b.op(), b.rhs());

                // 生成读取lhs的指令
                match get_valuedata!(lhs, ctx.func.unwrap()).kind() {
                    ValueKind::Integer(i) => {
                        writeln!(dest, "{}li a0, {}", " ".repeat(ctx.indent), i.value()).unwrap();
                    }
                    _ => {
                        let lhs_offset = match get_allocation!(&lhs, ctx) {
                            Allocation::Spilled(offset) => offset,
                            _ => unimplemented!(),
                        };
                        writeln!(dest, "{}lw a0, {}(sp)", " ".repeat(ctx.indent), lhs_offset)
                            .unwrap();
                    }
                }

                // 生成读取rhs的指令
                match get_valuedata!(rhs, ctx.func.unwrap()).kind() {
                    ValueKind::Integer(i) => {
                        writeln!(dest, "{}li a1, {}", " ".repeat(ctx.indent), i.value()).unwrap();
                    }
                    _ => {
                        let rhs_offset = match get_allocation!(&rhs, ctx) {
                            Allocation::Spilled(offset) => offset,
                            _ => unimplemented!(),
                        };
                        writeln!(dest, "{}lw a1, {}(sp)", " ".repeat(ctx.indent), rhs_offset)
                            .unwrap();
                    }
                }

                // 生成运算指令
                let self_offset = match get_allocation!(&self, ctx) {
                    Allocation::Spilled(offset) => offset,
                    _ => unimplemented!(),
                };
                match op {
                    // 算数操作代码生成
                    BinaryOp::Add => {
                        writeln!(dest, "{}add a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::Sub => {
                        writeln!(dest, "{}sub a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::Mul => {
                        writeln!(dest, "{}mul a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::Div => {
                        writeln!(dest, "{}div a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::Mod => {
                        writeln!(dest, "{}rem a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }

                    // 比较操作代码生成
                    BinaryOp::Lt => {
                        writeln!(dest, "{}slt a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::Le => {
                        writeln!(dest, "{}slt a0, a1, a0", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}not a0, a0", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::Eq => {
                        writeln!(dest, "{}xor a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}seqz a0, a0", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::Ge => {
                        writeln!(dest, "{}slt a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}not a0, a0", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::Gt => {
                        writeln!(dest, "{}slt a0, a1, a0", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::NotEq => {
                        writeln!(dest, "{}xor a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}snez a0, a0", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }

                    // 逻辑操作代码生成
                    BinaryOp::And=>{
                        writeln!(dest, "{}and a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }
                    BinaryOp::Or=>{
                        writeln!(dest, "{}or a0, a0, a1", " ".repeat(ctx.indent)).unwrap();
                        writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), self_offset)
                            .unwrap();
                    }

                    _ => todo!(),
                }

                ResultValue::KoopaRegister(*self)
            }
            // ValueKind::Binary(b) => {
            //     let (lhs, op, rhs) = (b.lhs(), b.op(), b.rhs());
            //     let lhs_valuedata = get_valuedata!(lhs, ctx.func.unwrap());
            //     let rhs_valuedata = get_valuedata!(rhs, ctx.func.unwrap());
            //     let lhs_is_koopa_reg = is_koopa_reg(lhs, ctx.func.unwrap());
            //     let rhs_is_koopa_reg = is_koopa_reg(rhs, ctx.func.unwrap());

            //     // lhs和rhs分配到的的riscv寄存器
            //     let lhs_riscv_reg_str =
            //         match ctx.alloc_result.clone().unwrap().allocs.get(&lhs).unwrap() {
            //             Allocation::Register(reg) => reg.to_string(),
            //             _ => unimplemented!(),
            //         };
            //     let rhs_riscv_reg_str =
            //         match ctx.alloc_result.clone().unwrap().allocs.get(&rhs).unwrap() {
            //             Allocation::Register(reg) => reg.to_string(),
            //             _ => unimplemented!(),
            //         };

            //     // 当lhs和rhs为integer时用这些
            //     let lhs_int = match lhs_valuedata.kind() {
            //         ValueKind::Integer(i) => Some(i.value()),
            //         _ => None,
            //     };
            //     let rhs_int = match rhs_valuedata.kind() {
            //         ValueKind::Integer(i) => Some(i.value()),
            //         _ => None,
            //     };

            //     // 自己一定是个koopa reg
            //     let self_riscv_reg =
            //         match ctx.alloc_result.clone().unwrap().allocs.get(&self).unwrap() {
            //             Allocation::Register(r) => r,
            //             _ => unimplemented!(),
            //         };
            //     let self_str = self_riscv_reg.to_string();

            //     match (lhs_is_koopa_reg, op, rhs_is_koopa_reg) {
            //         // 加
            //         (false, BinaryOp::Add, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 (lhs_int.unwrap() + rhs_int.unwrap()).to_string()
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Add, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}addi {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_int.unwrap().to_string()
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Add, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}addi {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 rhs_riscv_reg_str,
            //                 lhs_int.unwrap().to_string(),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Add, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}add {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str
            //             )
            //             .unwrap();
            //         }

            //         // 减
            //         (false, BinaryOp::Sub, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 (lhs_int.unwrap() - rhs_int.unwrap()).to_string(),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Sub, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}addi {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 (-rhs_int.unwrap()).to_string(),
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Sub, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 lhs_riscv_reg_str,
            //                 lhs_int.unwrap().to_string(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}sub {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Sub, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}sub {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str
            //             )
            //             .unwrap();
            //         }

            //         // 乘
            //         (false, BinaryOp::Mul, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 (lhs_int.unwrap() * rhs_int.unwrap()).to_string(),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Mul, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 rhs_riscv_reg_str,
            //                 rhs_int.unwrap().to_string(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}mul {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Mul, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 lhs_riscv_reg_str,
            //                 lhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}mul {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Mul, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}mul {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str
            //             )
            //             .unwrap();
            //         }

            //         // 除
            //         (false, BinaryOp::Div, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 (lhs_int.unwrap() / rhs_int.unwrap()).to_string(),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Div, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 rhs_riscv_reg_str,
            //                 rhs_int.unwrap().to_string(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}div {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Div, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 lhs_riscv_reg_str,
            //                 lhs_int.unwrap().to_string(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}div {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Div, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}div {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_riscv_reg.to_string(),
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str
            //             )
            //             .unwrap();
            //         }

            //         // 取余
            //         (false, BinaryOp::Mod, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_int.unwrap() % rhs_int.unwrap(),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Mod, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 rhs_riscv_reg_str,
            //                 rhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}rem {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Mod, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 lhs_riscv_reg_str,
            //                 lhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}rem {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Mod, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}rem {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }

            //         // 相等
            //         (false, BinaryOp::Eq, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 i32::from(lhs_int.unwrap() == rhs_int.unwrap()),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Eq, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}xori {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}seqz {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Eq, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}xori {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 rhs_riscv_reg_str,
            //                 lhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}seqz {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Eq, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}xor {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}seqz {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }

            //         // 不相等
            //         (false, BinaryOp::NotEq, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 i32::from(lhs_int.unwrap() != rhs_int.unwrap()),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::NotEq, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}xori {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}snez {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::NotEq, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}xori {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 rhs_riscv_reg_str,
            //                 lhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}snez {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::NotEq, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}xor {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}snez {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }

            //         // 小于
            //         (false, BinaryOp::Lt, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 i32::from(lhs_int.unwrap() < rhs_int.unwrap()),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Lt, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}slti {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_int.unwrap()
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Lt, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 lhs_riscv_reg_str,
            //                 lhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Lt, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str
            //             )
            //             .unwrap();
            //         }

            //         // 小于等于
            //         (false, BinaryOp::Le, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 i32::from(lhs_int.unwrap() <= rhs_int.unwrap()),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Le, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 rhs_riscv_reg_str,
            //                 rhs_int.unwrap()
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 rhs_riscv_reg_str,
            //                 lhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}not {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Le, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 lhs_riscv_reg_str,
            //                 lhs_int.unwrap()
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 rhs_riscv_reg_str,
            //                 lhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}not {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Le, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str
            //             )
            //             .unwrap();
            //         }

            //         // 大于等于
            //         (false, BinaryOp::Ge, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 i32::from(lhs_int.unwrap() >= rhs_int.unwrap()),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Ge, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}slti {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}not {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Ge, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 lhs_riscv_reg_str,
            //                 lhs_int.unwrap()
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}not {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Ge, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}not {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 self_str,
            //             )
            //             .unwrap();
            //         }

            //         // 大于
            //         (false, BinaryOp::Gt, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 i32::from(lhs_int.unwrap() > rhs_int.unwrap()),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Gt, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 rhs_riscv_reg_str,
            //                 rhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 rhs_riscv_reg_str,
            //                 lhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Gt, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 lhs_riscv_reg_str,
            //                 lhs_int.unwrap(),
            //             )
            //             .unwrap();
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 rhs_riscv_reg_str,
            //                 lhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Gt, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}slt {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 rhs_riscv_reg_str,
            //                 lhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }

            //         // 按位与
            //         (false, BinaryOp::And, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 i32::from(lhs_int.unwrap() & rhs_int.unwrap()),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::And, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}andi {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_int.unwrap(),
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::And, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}andi {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 rhs_riscv_reg_str,
            //                 lhs_int.unwrap(),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::And, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}and {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }

            //         // 按位或
            //         (false, BinaryOp::Or, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}li {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 i32::from(lhs_int.unwrap() | rhs_int.unwrap()),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Or, false) => {
            //             writeln!(
            //                 dest,
            //                 "{}ori {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_int.unwrap(),
            //             )
            //             .unwrap();
            //         }
            //         (false, BinaryOp::Or, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}ori {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 rhs_riscv_reg_str,
            //                 lhs_int.unwrap(),
            //             )
            //             .unwrap();
            //         }
            //         (true, BinaryOp::Or, true) => {
            //             writeln!(
            //                 dest,
            //                 "{}or {}, {}, {}",
            //                 " ".repeat(ctx.indent),
            //                 self_str,
            //                 lhs_riscv_reg_str,
            //                 rhs_riscv_reg_str,
            //             )
            //             .unwrap();
            //         }

            //         _ => unimplemented!(),
            //     };

            //     ResultValue::KoopaRegister(*self)
            // }
            ValueKind::Alloc(_) => {
                // 无需实现，已经在函数的epilogue分配好栈内存
                ResultValue::None
            }
            ValueKind::Load(l) => {
                // 生成读取指令
                let src = l.src();
                let src_offset = match ctx.alloc_result.as_ref().unwrap().allocs.get(&src).unwrap()
                {
                    Allocation::Spilled(offset) => *offset,
                    _ => unimplemented!(),
                };
                writeln!(dest, "{}lw a0, {}(sp)", " ".repeat(ctx.indent), src_offset).unwrap();

                // 生成存储指令
                let dest_offset = match ctx
                    .alloc_result
                    .as_ref()
                    .unwrap()
                    .allocs
                    .get(&self)
                    .unwrap()
                {
                    Allocation::Spilled(offset) => *offset,
                    _ => unimplemented!(),
                };
                writeln!(dest, "{}sw a0, {}(sp)", " ".repeat(ctx.indent), dest_offset).unwrap();

                ResultValue::KoopaRegister(*self)
            }
            ValueKind::Store(s) => {
                let (value, dest_mem) = (s.value(), s.dest());

                // 生成读取指令
                if !is_koopa_reg(value, ctx.func.unwrap()) {
                    let i = match ctx.func.unwrap().dfg().value(value).kind() {
                        ValueKind::Integer(i) => i.value(),
                        _ => unimplemented!(),
                    };
                    writeln!(dest, "{}li a0, {}", " ".repeat(ctx.indent), i,).unwrap();
                } else {
                    let value_offset = match ctx
                        .alloc_result
                        .as_ref()
                        .unwrap()
                        .allocs
                        .get(&value)
                        .unwrap()
                    {
                        Allocation::Spilled(off) => *off,
                        _ => unimplemented!(),
                    };
                    writeln!(
                        dest,
                        "{}lw a0, {}(sp)",
                        " ".repeat(ctx.indent),
                        value_offset,
                    )
                    .unwrap();
                }

                // 生成存储指令
                let dest_mem_offset = match ctx
                    .alloc_result
                    .as_ref()
                    .unwrap()
                    .allocs
                    .get(&dest_mem)
                    .unwrap()
                {
                    Allocation::Spilled(off) => *off,
                    _ => unimplemented!(),
                };
                writeln!(
                    dest,
                    "{}sw a0, {}(sp)",
                    " ".repeat(ctx.indent),
                    dest_mem_offset
                )
                .unwrap();

                ResultValue::None
            }
            _ => unimplemented!(),
        }
    }
}

fn is_koopa_reg(value: Value, func_data: &FunctionData) -> bool {
    let value_data = func_data.dfg().value(value);
    let kind = value_data.kind();
    match kind {
        ValueKind::Integer(_) | ValueKind::Return(_) | ValueKind::Store(_) => false,
        _ => true,
    }
}
