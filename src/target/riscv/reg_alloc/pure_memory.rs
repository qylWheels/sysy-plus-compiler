//! 纯栈内存分配（除了函数参数，它们需要使用寄存器）

use std::collections::HashMap;

use koopa::ir::{Value, ValueKind};
use strum::IntoEnumIterator;

use crate::target::riscv::reg_alloc::{AllocResult, Allocation, RegAllocator, Register};

#[derive(Debug, Clone)]
pub(crate) struct PureMemoryAllocator;

impl PureMemoryAllocator {
    pub(crate) fn new() -> Self {
        Self
    }

    // /// 判断要为对应的value分配多少栈空间
    // fn need_alloc(value_data: &ValueData) -> usize {
    //     let value_kind = value_data.kind();
    //     match value_kind {
    //         ValueKind::Alloc(_) => 4,
    //         _ => 0,
    //     }
    // }
}

impl RegAllocator for PureMemoryAllocator {
    fn allocate(&mut self, func_data: &koopa::ir::FunctionData) -> AllocResult {
        // 存储value-allocation键值对
        let mut map = HashMap::new();

        // 记录总共要为该函数分配的栈空间
        let mut stack_size = 0usize;

        // 遍历函数，并：
        // 1、找出除了不需分配内存的指令（如ret、store）外的所有指令（如add、load）
        // 2、找出所有的call指令
        let mut need_allocs = vec![]; // 所有需要分配内存的指令
        let mut calls = vec![]; // call指令
        for (_, node) in func_data.layout().bbs() {
            for &value in node.insts().keys() { // FIXME: 遍历不到call指令是因为只对f()函数调用了此函数！
                let value_data = func_data.dfg().value(value);
                match value_data.kind() {
                    ValueKind::Store(_) | ValueKind::Return(_) | ValueKind::Integer(_) => (), // 不需分配
                    ValueKind::Call(call) => {
                        calls.push(call);
                        need_allocs.push(value); // call指令的返回值需要空间
                    }
                    _ => need_allocs.push(value),
                }
            }
        }

        // XXX: 暂时无需保存寄存器
        // // 1、收集callee-saved的，且需要在函数体内生成代码来保存的寄存器
        // // 2、收集caller-saved的，且需要在函数体内生成代码来保存的寄存器
        // let callee_saved = vec![
        //     Register::s0,
        //     Register::s1,
        //     Register::s2,
        //     Register::s3,
        //     Register::s4,
        //     Register::s5,
        //     Register::s6,
        //     Register::s7,
        //     Register::s8,
        //     Register::s9,
        //     Register::s10,
        //     Register::s11,
        // ];
        // let caller_saved = vec![

        // ];

        // 计算总共所需栈空间
        stack_size += need_allocs.len() * 4; // 所有需要分配内存的指令所需的栈空间
        stack_size += if calls.is_empty() { 0 } else { 4 }; // 保存ra所需栈空间
        let max_arg_count = calls
            .iter()
            .map(|call| call.args().len())
            .max()
            .unwrap_or(0);
        stack_size += (*[(max_arg_count as isize) - 8, 0].iter().max().unwrap() as usize) * 4; // 参数所需最大空间
        stack_size = (stack_size + 15) / 16 * 16; // 按16的倍数向上取整

        // 分配空间。注意：
        // 1、sp + stack_size - 4是给ra留的
        // 2、前八个参数应该对应寄存器。八个之后的参数应该从低地址到高地址存放
        let mut pointer = 0; // 为value分配空间时指向可用空间的下一个内存地址
        pointer += stack_size; // 将pointer置于栈底
        pointer -= if calls.is_empty() { 0 } else { 4 }; // 为ra预留空间

        // 为本函数的形参“分配”空间
        // 实际上本函数的形参所占用的空间并不在本函数栈内，而是在寄存器a0~a7（前8个）/caller的栈内（剩余的）
        for (i, arg) in func_data.params().iter().enumerate() {
            if i < 8 {
                map.entry(*arg)
                    .and_modify(|_| panic!("register is already allocated for {arg:?}"))
                    .or_insert(Allocation::Register(Register::iter().nth(i).unwrap()));
            } else {
                map.entry(*arg)
                    .and_modify(|_| panic!("stack memory is already allocated for {arg:?}"))
                    .or_insert(Allocation::Spilled(stack_size + (i - 8) * 4));
            }
        }

        // 为value分配栈空间
        for need_alloc in need_allocs {
            pointer -= 4;
            map.entry(need_alloc)
                .and_modify(|_| panic!("stack memory is already allocated for {need_alloc:?}"))
                .or_insert(Allocation::Spilled(pointer));
        }

        // XXX: 为调用函数时的实参分配空间的工作由asmgen模块负责，本模块只负责开辟足够的空间给实参
        // 若在这里分配空间，则会出现重复分配空间的错误。
        // for &call in &calls {
        //     let args = call.args();
        //     for (i, arg) in args.iter().enumerate() {
        //         let arg = *arg;
        //         if i < 8 {
        //             map.entry(arg)
        //                 .and_modify(|_| panic!("register is already allocated for {arg:?}"))
        //                 .or_insert(Allocation::Register(Register::iter().nth(i).unwrap()));
        //         } else {
        //             map.entry(arg)
        //                 .and_modify(|_| panic!("stack memory is already allocated for {arg:?}"))
        //                 .or_insert(Allocation::Spilled(4 * (i - 8)));
        //         }
        //     }
        // }

        AllocResult {
            allocs: map,
            stack_size: stack_size,
            save_ra: !calls.is_empty(),
        }
    }
}
