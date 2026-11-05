use proc_macro2::TokenStream;
use std::{env, fmt::Write as _, fs, path::PathBuf};

mod analysis;
mod codegen;
mod ir;
mod parser;

fn dump(ir: &ir::Ir, an: &analysis::Analysis) -> String {
    let mut s = String::new();
    for st in &ir.states {
        let name = parser::state_name(st.id);
        if !an.is_live(st.id) {
            writeln!(s, "state {name}: unreachable (not emitted)").unwrap();
            continue;
        }
        let mut tag = if an.is_checked(st.id) {
            " [budget check]"
        } else {
            ""
        }
        .to_string();

        if an.is_inlined(st.id) {
            tag.push_str(" [inline]");
        }

        writeln!(s, "state {name}{tag}").unwrap();
        match &st.body {
            ir::Body::Straight(b) => writeln!(s, "  always: {b}").unwrap(),
            ir::Body::Switch {
                common,
                arms: [z, o],
            } => {
                if !common.is_empty() {
                    writeln!(s, "  hoisted: {}", ir::fmt_ops(common)).unwrap();
                }
                writeln!(s, "  read 0: {z}\n  read 1: {o}").unwrap();
            }
            ir::Body::ScanThen { run, rest } => {
                writeln!(s, "  read {}: {run}", run.read as u8).unwrap();
                writeln!(s, "  then (read {}): {rest}", !run.read as u8).unwrap();
            }
        }
    }
    s
}

fn main() {
    let src = env::var("MACHINE").expect("Must have a machine set");

    let program = parser::parse(&src);
    let ir = ir::lower(&program);
    let analysis = analysis::analyze(&ir);
    let tokens: TokenStream = codegen::emit(&ir, &analysis, &src);

    let file = syn::parse2(tokens).expect("generated invalid Rust");
    let formatted = prettyplease::unparse(&file);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("machine.rs"), formatted).unwrap();

    if env::var_os("DUMP_IR").is_some() {
        let report = dump(&ir, &analysis);
        fs::write(out_dir.join("machine.ir.txt"), &report).unwrap();

        for line in report.lines() {
            println!("cargo::warning={line}");
        }
    }

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=MACHINE");
    println!("cargo::rerun-if-env-changed=DUMP_IR");
}
