use std::io;

use koopa::{back::KoopaGenerator, ir::Program};

pub fn irgen(prog: &Program, output: impl io::Write) {
    let _ = KoopaGenerator::new(output).generate_on(prog);
}
