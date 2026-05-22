use std::sync::{Mutex, OnceLock};

use crate::{
    parser::ast::common::Type,
    semantic::symbol_table::{Qualifier, SymbolInfo},
};

static BUILTIN_FUNCTIONS: OnceLock<Mutex<Vec<(String, SymbolInfo)>>> = OnceLock::new();

// TODO: 修改内置函数表，使其不必遵循sysy的要求
pub(crate) fn get_builtin_functions() -> &'static Mutex<Vec<(String, SymbolInfo)>> {
    BUILTIN_FUNCTIONS.get_or_init(|| {
        Mutex::new(vec![
            (
                "getint".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(vec![], Box::new(Type::Simple("int".to_string()))),
                },
            ),
            (
                "getch".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(vec![], Box::new(Type::Simple("int".to_string()))),
                },
            ),
            (
                "getarray".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(
                        vec![Box::new(Type::Pointer(Box::new(Type::Simple(
                            "int".to_string(),
                        ))))],
                        Box::new(Type::Simple("int".to_string())),
                    ),
                },
            ),
            (
                "putint".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(
                        vec![Box::new(Type::Simple("int".to_string()))],
                        Box::new(Type::Void),
                    ),
                },
            ),
            (
                "putch".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(
                        vec![Box::new(Type::Simple("int".to_string()))],
                        Box::new(Type::Void),
                    ),
                },
            ),
            (
                "putarray".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(
                        vec![
                            Box::new(Type::Simple("int".to_string())),
                            Box::new(Type::Pointer(Box::new(Type::Simple("int".to_string())))),
                        ],
                        Box::new(Type::Void),
                    ),
                },
            ),
            (
                "starttime".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(vec![], Box::new(Type::Void)),
                },
            ),
            (
                "stoptime".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(vec![], Box::new(Type::Void)),
                },
            ),
        ])
    })
}
