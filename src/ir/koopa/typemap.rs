use koopa::ir;

use crate::parser::ast::common;

pub fn typemap(sysy_type: &common::Type) -> ir::Type {
    match sysy_type {
        common::Type::Void => ir::Type::get_unit(),
        common::Type::Simple(_) => ir::Type::get_i32(),
        common::Type::Pointer(ty) => {
            let koopa_ty = typemap(&ty);
            ir::Type::get_pointer(koopa_ty)
        }
        common::Type::Function(param_tys, ret_ty) => {
            let param_koopa_tys: Vec<ir::Type> = param_tys.iter().map(|ty| typemap(ty)).collect();
            // dbg!(&param_koopa_tys);
            let ret_koopa_ty = typemap(ret_ty);
            // dbg!(&ret_koopa_ty);
            ir::Type::get_function(param_koopa_tys, ret_koopa_ty)
        }
    }
}
