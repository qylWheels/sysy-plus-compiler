use inkwell::context::Context;
use std::env;
use std::fs;
use std::path::PathBuf;
use sysy_compiler::ir;
use sysy_compiler::ir::inkwell::builder::IrGenerator;
use sysy_compiler::parser::grammar;
use sysy_compiler::semantic::check::SemanticChecker;

#[derive(Debug, Clone)]
struct Cli {
    /// 生成的目标代码
    pub target: String,

    /// 输入路径
    pub input: PathBuf,

    /// 输出路径
    pub output: PathBuf,
}

fn main() {
    let cli = parse_args();

    let i = fs::read_to_string(cli.input).unwrap();
    let mut o = fs::File::create(cli.output).unwrap();

    // 语法分析
    let parser = grammar::CompileUnitParser::new();
    let compunit = parser.parse(&i).unwrap();
    // dbg!(&compunit);

    // 语义检查
    let mut checker = SemanticChecker::new();
    checker.check(&compunit).unwrap();
    // dbg!("checked!");

    // IR生成
    let ctx = Context::create();
    let ir_generator = IrGenerator::new(&ctx);
    let module = ir_generator.build_compunit(&compunit).unwrap();
    module.print_to_stderr();
    // match cli.target.as_ref() {
    //     "-koopa" => irgen::irgen(&program, o),
    //     "-riscv" => {
    //         let _ = program.generate(&mut o, Context::new());
    //     }
    //     _ => unimplemented!(),
    // }
}

fn parse_args() -> Cli {
    let args: Vec<String> = env::args().collect();
    Cli {
        target: args[1].clone(),
        input: PathBuf::from(args[2].clone()),
        output: PathBuf::from(args[4].clone()),
    }
}
