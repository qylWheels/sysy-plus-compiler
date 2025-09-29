//! 线性扫描算法

use koopa::ir::{entities::ValueData, FunctionData, Value, ValueKind};
use std::{collections::*, hash::Hash, ops::Range};
use strum::IntoEnumIterator;

/// 寄存器
#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Eq, strum_macros::EnumIter, strum_macros::Display,
)]
#[allow(non_camel_case_types)]
pub enum Register {
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
    t7,
}

/// 寄存器状态
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RegStatus {
    Unused,
    Used,
}

/// 分配类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Allocation {
    /// 寄存器
    Register(Register),

    /// 溢出，usize是栈偏移
    Spilled(usize),
}

/// Value的生命周期
#[derive(Debug, Clone)]
struct ValueLifeRange {
    value: Value,
    life_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct RegAllocator {
    /// IR中所有Value的生命周期
    life_ranges: Vec<ValueLifeRange>,

    /// 可用寄存器池
    available_regs: VecDeque<Register>,

    /// 记录每个值分配到了什么
    reg_allocs: HashMap<Value, Allocation>,
}

impl RegAllocator {
    pub fn new() -> Self {
        Self {
            life_ranges: vec![],
            available_regs: {
                let mut deque = VecDeque::new();
                for reg in Register::iter() {
                    deque.push_back(reg);
                }
                deque
            },
            reg_allocs: HashMap::new(),
        }
    }

    fn scan(&mut self, func_data: &FunctionData) {
        let mut map: HashMap<Value, Range<usize>> = HashMap::new();
        let mut counter = 0;
        for (_, node) in func_data.layout().bbs() {
            for &value in node.insts().keys() {
                let value_data = func_data.dfg().value(value);
                if Self::need_alloc_reg(&value_data) {
                    map.entry(value)
                        .and_modify(|r| r.end = counter)
                        .or_insert(counter..usize::MAX);
                }
                match value_data.kind() {
                    ValueKind::Binary(b) => {
                        let (lhs, rhs) = (b.lhs(), b.rhs());
                        if Self::need_alloc_reg(func_data.dfg().value(lhs)) {
                            map.entry(lhs)
                                .and_modify(|r| r.end = counter)
                                .or_insert(counter..usize::MAX);
                        }
                        if Self::need_alloc_reg(func_data.dfg().value(rhs)) {
                            map.entry(rhs)
                                .and_modify(|r| r.end = counter)
                                .or_insert(counter..usize::MAX);
                        }
                    }
                    ValueKind::Return(r) => {
                        let value = r.value();
                        match value {
                            Some(v) => {
                                if Self::need_alloc_reg(func_data.dfg().value(v)) {
                                    map.entry(v)
                                        .and_modify(|r| r.end = counter)
                                        .or_insert(counter..usize::MAX);
                                }
                            }
                            None => (),
                        }
                    }
                    _ => unimplemented!(),
                }
                counter += 1;
            }
        }
        self.life_ranges
            .extend(map.iter().map(|(value, life_range)| ValueLifeRange {
                value: *value,
                life_range: life_range.clone(),
            }))
    }

    // TODO: 实现寄存器释放和变量溢出
    pub fn allocate(&mut self, func_data: &FunctionData) -> &HashMap<Value, Allocation> {
        // 扫描函数中的Value，确定每个Value的生命周期
        self.scan(func_data);

        // 将生命周期按起始顺序先后排序
        self.life_ranges.sort_by_key(|k| k.life_range.end);

        // 为每一个Value分配寄存器
        let cloned_life_ranges = self.life_ranges.clone();
        for ValueLifeRange {
            value,
            life_range: _,
        } in cloned_life_ranges
        {
            let reg = self.alloc_reg();
            self.reg_allocs
                .entry(value)
                .or_insert(Allocation::Register(reg));
        }

        // dbg!(&self.reg_allocs);
        &self.reg_allocs
    }

    fn need_alloc_reg(value_data: &ValueData) -> bool {
        let kind = value_data.kind();
        match kind {
            ValueKind::Return(_) => false,
            _ => true,
        }
    }

    fn alloc_reg(&mut self) -> Register {
        self.available_regs.pop_front().unwrap()
    }

    #[allow(dead_code)]
    fn free_reg(&mut self) {
        todo!()
    }
}
