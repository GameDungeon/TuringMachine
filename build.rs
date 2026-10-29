use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::{env, fs, path::PathBuf};

enum Instruction {
    Instruction {
        symbol: bool,
        direction: bool,
        state: char,
    },
    Halt,
}

fn parse_instruction(s: &str) -> Instruction {
    let mut chars = s.chars();

    let symbol = match chars.next().unwrap() {
        '-' => {
            return Instruction::Halt;
        }
        '0' => false,
        '1' => true,
        c => panic!("invalid symbol: {c}"),
    };

    let direction = match chars.next().unwrap() {
        'L' => false,
        'R' => true,
        c => panic!("invalid direction: {c}"),
    };

    let state = chars.next().unwrap();

    Instruction::Instruction {
        symbol,
        direction,
        state,
    }
}

fn parse_program(input: &str) -> Vec<(Instruction, Instruction)> {
    input
        .split('_')
        .map(|group| {
            let (zero, one) = group.split_at(3);
            (parse_instruction(zero), parse_instruction(one))
        })
        .collect()
}

fn generate_state_inner(instruction: &Instruction, cur_symbol: bool) -> TokenStream {
    match instruction {
        Instruction::Instruction {
            symbol,
            direction,
            state,
        } => {
            let write_symbol = if cur_symbol == *symbol {
                // No need to write a symbol that matches the tape
                quote! {}
            } else {
                quote! { head.write::<#symbol>(); }
            };

            if *state == 'Z' {
                quote! { step_budget - 1 }
            } else {
                let next_state = format_ident!("state_{}", &state.to_ascii_lowercase());
                quote! {
                    #write_symbol
                    if head.shift::<#direction>() { tape.cross::<#direction>(&mut head); }
                    become #next_state(head, tape, step_budget - 1)
                }
            }
        }
        Instruction::Halt => quote! { step_budget - 1 },
    }
}

fn generate_state_function(instructions: &(Instruction, Instruction), state: &str) -> TokenStream {
    let inner_zero = generate_state_inner(&instructions.0, false);
    let inner_one = generate_state_inner(&instructions.1, true);

    let fn_name = format_ident!("state_{}", state);

    quote! {
        fn #fn_name(mut head: Head, tape: &mut Tape, step_budget: u64) -> u64 {
            if step_budget == 0 { return 0; }

            if head.read() {
                #inner_one
            } else {
                #inner_zero
            }
        }
    }
}

fn generate_machine(instructions: Vec<(Instruction, Instruction)>) -> TokenStream {
    let states = ["a", "b", "c", "d", "e", "f", "g", "h"];

    let state_functions = instructions
        .iter()
        .enumerate()
        .map(|(i, inst)| generate_state_function(inst, states[i]));

    quote! {
        pub fn run_machine(head: Head, tape: &mut Tape, step_budget: u64) -> u64 {
            step_budget - state_a(head, tape, step_budget)
        }

        #(#state_functions)*
    }
}

fn main() {
    let machine = std::env::var("MACHINE").expect("Must have a machine set");

    let program = parse_program(&machine);
    let tokens = generate_machine(program);

    let file = syn::parse2(tokens).expect("generated invalid Rust");
    let formatted = prettyplease::unparse(&file);

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("machine.rs");
    fs::write(out, formatted).unwrap();

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=MACHINE");
}
