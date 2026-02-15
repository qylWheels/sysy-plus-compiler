use std::sync::{Mutex, OnceLock};

use crate::{
    parser::ast::common::Type,
    semantic::symbol_table::{Qualifier, SymbolInfo},
};

static BUILTIN_FUNCTIONS: OnceLock<Mutex<Vec<(String, SymbolInfo)>>> = OnceLock::new();

pub(crate) fn get_builtin_functions() -> &'static Mutex<Vec<(String, SymbolInfo)>> {
    BUILTIN_FUNCTIONS.get_or_init(|| {
        Mutex::new(vec![
            (
                "getint".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(vec![], Box::new(Type::Simple("int".to_string()))),
                    const_val: None,
                },
            ),
            (
                "getch".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(vec![], Box::new(Type::Simple("int".to_string()))),
                    const_val: None,
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
                    const_val: None,
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
                    const_val: None,
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
                    const_val: None,
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
                    const_val: None,
                },
            ),
            (
                "starttime".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(vec![], Box::new(Type::Void)),
                    const_val: None,
                },
            ),
            (
                "stoptime".to_string(),
                SymbolInfo {
                    qualifier: Qualifier::Const,
                    ty: Type::Function(vec![], Box::new(Type::Void)),
                    const_val: None,
                },
            ),
        ])
    })
}
