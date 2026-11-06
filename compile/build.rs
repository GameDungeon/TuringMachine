use proc_macro2::TokenStream;
use std::{env, fs, path::PathBuf};

mod checks;
mod codegen;
mod inline;
mod ir;
mod optimizer;
mod parser;

fn main() {
    // Take in MACHINE env var or just use Anti-Hydra
    let src =
        env::var("MACHINE").unwrap_or("1RB1RA_0LC1LE_1LD1LC_1LA0LB_1LF1RE_---0RA".to_string());

    let program = parser::parse(&src);

    let mut ir = ir::lower(&program);
    inline::run(&mut ir);
    optimizer::optimize_ir(&mut ir);
    checks::place(&mut ir);

    let tokens: TokenStream = codegen::emit(&ir, &src);

    let file = syn::parse2(tokens).expect("generated invalid Rust");
    let formatted = prettyplease::unparse(&file);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("machine.rs"), formatted).unwrap();

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=MACHINE");
    println!("cargo::rerun-if-env-changed=DUMP_IR");
}
