use std::env;
use std::fs;
use sysy_compiler::ir::irgen::irgen;
use sysy_compiler::ir::program_builder;
use sysy_compiler::parser::grammar;

fn main() {
    let (i, o) = parse_args();
    let parser = grammar::CompUnitParser::new();
    let compunit = parser.parse(&i);
    let program = program_builder::ProgramBuilder::new(compunit.unwrap()).build_compunit();
    irgen(&program, o);
}

fn parse_args() -> (String, fs::File) {
    let args: Vec<String> = env::args().collect();
    if args[1] == "-koopa".to_string() && args[3] == "-o" {
        (
            fs::read_to_string(&args[2]).unwrap(),
            fs::File::create(&args[4]).unwrap(),
        )
    } else {
        panic!("Invalid argument(s)")
    }
}
