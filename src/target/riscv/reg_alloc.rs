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
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ValueLifeRange {
    value: Value,
    life_range: Range<usize>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RegAllocator {
    /// IR中所有需要分配的Value的生命周期
    life_ranges: Vec<ValueLifeRange>,

    /// 可用寄存器池
    available_regs: VecDeque<Register>,

    /// 记录每个值分配到了什么
    allocs: HashMap<Value, Allocation>,
}

// TODO: 目前的“寄存器分配”算法只会分配内存，后续要改成能分配寄存器的
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
            allocs: HashMap::new(),
        }
    }

    fn scan(&mut self, func_data: &FunctionData) {
        let mut map: HashMap<Value, Range<usize>> = HashMap::new();
        let mut pos = 0;
        for (_, node) in func_data.layout().bbs() {
            for &value in node.insts().keys() {
                let value_data = func_data.dfg().value(value);
                if Self::need_alloc(&value_data) {
                    map.entry(value)
                        .and_modify(|r| r.end = pos)
                        .or_insert(pos..usize::MAX);
                }
                match value_data.kind() {
                    ValueKind::Binary(b) => {
                        let (lhs, rhs) = (b.lhs(), b.rhs());
                        if Self::need_alloc(func_data.dfg().value(lhs)) {
                            map.entry(lhs)
                                .and_modify(|r| r.end = pos)
                                .or_insert(pos..usize::MAX);
                        }
                        if Self::need_alloc(func_data.dfg().value(rhs)) {
                            map.entry(rhs)
                                .and_modify(|r| r.end = pos)
                                .or_insert(pos..usize::MAX);
                        }
                    }
                    ValueKind::Return(r) => {
                        let value = r.value();
                        match value {
                            Some(v) => {
                                if Self::need_alloc(func_data.dfg().value(v)) {
                                    map.entry(v)
                                        .and_modify(|r| r.end = pos)
                                        .or_insert(pos..usize::MAX);
                                }
                            }
                            None => (),
                        }
                    }
                    ValueKind::Alloc(_) => {
                        // void
                    }
                    ValueKind::Store(s) => {
                        let (value, dest) = (s.value(), s.dest());
                        if Self::need_alloc(func_data.dfg().value(value)) {
                            map.entry(value)
                                .and_modify(|r| r.end = pos)
                                .or_insert(pos..usize::MAX);
                        }
                        if Self::need_alloc(func_data.dfg().value(dest)) {
                            map.entry(dest)
                                .and_modify(|r| r.end = pos)
                                .or_insert(pos..usize::MAX);
                        }
                    }
                    ValueKind::Load(l) => {
                        let src = l.src();
                        if Self::need_alloc(func_data.dfg().value(src)) {
                            map.entry(src)
                                .and_modify(|r| r.end = pos)
                                .or_insert(pos..usize::MAX);
                        }
                    }
                    _ => unimplemented!("{:?}", value_data.kind()),
                }
                pos += 1;
            }
        }
        self.life_ranges
            .extend(map.iter().map(|(value, life_range)| ValueLifeRange {
                value: *value,
                life_range: life_range.clone(),
            }))
    }

    // TODO: 实现寄存器释放和变量溢出
    pub fn allocate(&mut self, func_data: &FunctionData) -> AllocResult<'_> {
        // 扫描函数中的Value，确定每个Value的生命周期
        self.scan(func_data);

        // // 将生命周期按起始顺序先后排序
        // self.life_ranges.sort_by_key(|k| k.life_range.end);

        // // 为每一个Value分配寄存器
        // let cloned_life_ranges = self.life_ranges.clone();
        // for ValueLifeRange {
        //     value,
        //     life_range: _,
        // } in cloned_life_ranges
        // {
        //     let reg = self.alloc_reg();
        //     self.reg_allocs
        //         .entry(value)
        //         .or_insert(Allocation::Register(reg));
        // }

        // 为每个Value分配内存
        let mut offset = 0;
        let values = self.life_ranges.iter().map(|range| range.value);
        for value in values {
            self.allocs.insert(value, Allocation::Spilled(offset));
            offset += 4;
        }

        // dbg!(&self
        //     .life_ranges
        //     .iter()
        //     .map(|range| func_data.dfg().value(range.value))
        //     .collect::<Vec<_>>());

        AllocResult {
            allocs: &self.allocs,
            stack_size: (self.life_ranges.len() * 4 + 15) / 16 * 16, // 按16的倍数向上取整
        }
    }

    fn need_alloc(value_data: &ValueData) -> bool {
        let kind = value_data.kind();
        match kind {
            ValueKind::Return(_) | ValueKind::Store(_) | ValueKind::Integer(_) => false,
            _ => true,
        }
    }

    #[allow(dead_code)]
    fn alloc_reg(&mut self) -> Register {
        self.available_regs.pop_front().unwrap()
    }

    #[allow(dead_code)]
    fn free_reg(&mut self) {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct AllocResult<'a> {
    pub(crate) stack_size: usize, // 要求该值按16的倍数向上取整
    pub(crate) allocs: &'a HashMap<Value, Allocation>,
}
