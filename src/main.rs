use std::env;
use std::fs;
use std::path::PathBuf;
use sysy_compiler::ir::irgen;
use sysy_compiler::ir::program_builder;
use sysy_compiler::parser::grammar;
use sysy_compiler::semantic::symbol_table::SymbolTable;
use sysy_compiler::target::riscv::asmgen::Context;
use sysy_compiler::target::riscv::asmgen::GenerateRiscv;

#[derive(Debug, Clone)]
struct Cli {
    /// 生成的目标代码
    pub target: String,

    pub input: PathBuf,

    pub output: PathBuf,
}

fn main() {
    let cli = parse_args();

    let i = fs::read_to_string(cli.input).unwrap();
    let mut o = fs::File::create(cli.output).unwrap();

    // 语法分析
    let parser = grammar::CompUnitParser::new();
    let compunit = parser.parse(&i).unwrap();
    dbg!(&compunit);

    // 语义检查
    let mut symtable = SymbolTable::new();
    symtable.check(&compunit).unwrap(); // TODO: 使用anyhow处理错误

    // IR/目标代码生成
    let program = program_builder::ProgramBuilder::new(compunit, &symtable).build_compunit();
    match cli.target.as_ref() {
        "-koopa" => irgen::irgen(&program, o),
        "-riscv" => {
            let _ = program.generate(
                &mut o,
                Context {
                    func: None,
                    indent: 0,
                    reg_alloc_result: None,
                },
            );
        }
        _ => unimplemented!(),
    }
}

fn parse_args() -> Cli {
    let args: Vec<String> = env::args().collect();
    Cli {
        target: args[1].clone(),
        input: PathBuf::from(args[2].clone()),
        output: PathBuf::from(args[4].clone()),
    }
}
