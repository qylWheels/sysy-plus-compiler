use std::collections::HashMap;

use koopa::ir::{FunctionData, Value};

pub(super) mod linear_scan;
pub(super) mod pure_memory;

/// 寄存器
#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Eq, strum_macros::EnumIter, strum_macros::Display,
)]
#[allow(non_camel_case_types)]
pub(super) enum Register {
    a0,
    a1,
    a2,
    a3,
    a4,
    a5,
    a6,
    a7,

    t0,
    t1,
    t2,
    t3,
    t4,
    t5,
    t6,
}

/// 分配类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Allocation {
    /// 寄存器
    Register(Register),

    /// 栈溢出，usize是栈偏移
    Spilled(usize),
}

#[derive(Debug, Clone)]
pub(super) struct AllocResult {
    /// 总共需要分配的栈空间。该值需按16的倍数向上取整
    pub(super) stack_size: usize,

    /// 每个koopa value对应的分配
    pub(super) allocs: HashMap<Value, Allocation>,

    // XXX: 暂时无需实现
    // /// 自己作为callee时要保存的寄存器对应的分配
    // pub(super) callee_saved_regs: HashMap<Register, Allocation>,

    // /// 自己作为caller时要保存的寄存器对应的分配
    // pub(super) caller_saved_regs: HashMap<Register, Allocation>,
    /// 是否要保存ra（即是否调用了其他）
    pub(super) save_ra: bool,
}

pub(super) trait RegAllocator {
    fn allocate(&mut self, func_data: &FunctionData) -> AllocResult;
}
