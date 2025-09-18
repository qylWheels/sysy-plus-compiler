use koopa::ir;

use crate::parser::ast::common;

pub fn typemap(sysy_type: &common::Type) -> ir::Type {
    match sysy_type {
        common::Type::Simple(_) => ir::Type::get_i32(),
    }
}
