use std::marker::PhantomData;

use inkwell::{
    context::Context,
    types::{AnyType, AnyTypeEnum, BasicMetadataTypeEnum, BasicType, BasicTypeEnum},
    AddressSpace,
};

use crate::parser::ast::common::Type;

#[derive(Debug, Clone)]
pub(crate) struct TypeMapper<'ctx> {
    _marker: PhantomData<&'ctx ()>,
}

impl<'ctx> TypeMapper<'ctx> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    pub fn map(&self, sysy_type: &Type, llvm_ctx: &'ctx Context) -> BasicTypeEnum<'ctx> {
        match sysy_type {
            Type::Void => llvm_ctx.struct_type(&[], true).as_basic_type_enum(),
            Type::Simple(typename) => match typename.as_str() {
                "int" => llvm_ctx.i32_type().as_basic_type_enum(),
                _ => unimplemented!(),
            },
            Type::Pointer(_p) => todo!(),

            // 检查function类型是在语义检查该做的事
            // ir生成阶段没有函数类型，只有指针类型
            Type::Function(_arg_tys, _ret_ty) => llvm_ctx
                .ptr_type(AddressSpace::default())
                .as_basic_type_enum(),
        }
    }
}
