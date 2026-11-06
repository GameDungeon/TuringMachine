use std::env;

use crate::inline;
use crate::ir::*;
use crate::parser::state_name;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

fn fn_ident(s: &StateFn) -> Ident {
    format_ident!("state_{}{}", state_name(s.state), s.ctx.suffix())
}

/// Halt code = state * 2 + symbol. `Z` (u8::MAX) means "out of budget".
fn halt_code(state: StateId, symbol: Sym) -> u8 {
    (state * 2 + symbol as usize) as u8
}

pub fn emit(ir: &Ir, machine: &str) -> TokenStream {
    let names: Vec<String> = (0..ir.n_states)
        .map(|i| state_name(i).to_string().to_uppercase())
        .collect();
    let n = names.len();
    let live = inline::reachable(ir);
    let fn_names: Vec<Ident> = ir.states.iter().map(fn_ident).collect();
    let fns: Vec<TokenStream> = ir
        .states
        .iter()
        .filter(|s| live[s.id])
        .map(|s| emit_state(s, &fn_names))
        .collect();
    let entry = &fn_names[0];

    let steps = env::var_os("STEPS")
        .map(|x| x.to_str().unwrap().parse().unwrap())
        .unwrap_or(10u64.pow(10))
        + 20;

    assert!(steps >= 100, "Must perform at least 100 steps");

    let write_out = env::var_os("OUTPUT_FILE")
        .map(|x| x.into_string().expect("Sting must be unicode"))
        .unwrap_or("".to_string());

    quote! {
        const MACHINE: &str = #machine;
        const HOLDOUT: u8 = u8::MAX;
        const NAMES: [&str; #n] = [#(#names),*];
        const STEPS: u64 = #steps;
        const OUTPUT: &str = #write_out;

        pub fn run_machine(
            head: Head,
            tape: &mut Tape
        ) -> (u64, &'static str, bool) {
            let (left, code) = #entry(head, tape, STEPS);

            if code == HOLDOUT {
                (0, "HOLDOUT", false)
            } else {
                (STEPS - left, NAMES[(code >> 1) as usize], code & 1 != 0)
            }
        }

        #(#fns)*
    }
}

fn emit_ops(ops: &[Op]) -> Vec<TokenStream> {
    ops.iter()
        .map(|op| match *op {
            Op::Flip => quote! { head.flip(); },
            Op::Write(s) => quote! { head.write::<#s>(); },
            Op::Shift(dir) => quote! { head.shift::<#dir>(tape); },
            Op::Step(n) => quote! { step_budget -= #n; },
            Op::Check(t) => quote! { if step_budget <= #t { return (0, HOLDOUT); } },
        })
        .collect()
}

fn emit_block(b: &Block, names: &[Ident]) -> TokenStream {
    let ops = emit_ops(&b.ops);
    let term = match &b.term {
        Term::Tail(n) => {
            let f = &names[*n];
            quote! { become #f(head, tape, step_budget) }
        }
        Term::Halt { state, symbol } => {
            let code = halt_code(*state, *symbol);
            quote! { return (step_budget, #code) }
        }
        Term::Branch { arms } => {
            let a0 = emit_block(&arms[0], names);
            let a1 = emit_block(&arms[1], names);
            quote! { if head.read() { #a1 } else { #a0 } }
        }
    };
    quote! { #(#ops)* #term }
}

fn emit_state(s: &StateFn, names: &[Ident]) -> TokenStream {
    let name = names[s.id].clone();

    let body = match &s.body {
        Body::Code(b) => emit_block(b, names),
        Body::ScanThen { run, rest, post } => {
            let Run { dir, read, write } = *run;
            let method = format_ident!("shift_rule");
            let cond = if read == Sym::S1 {
                quote! { head.read() }
            } else {
                quote! { !head.read() }
            };
            let rest = emit_block(rest, names);
            quote! {
                if #cond {
                    step_budget = head.#method::<#dir, #read, #write>(tape, step_budget);
                    if step_budget <= #post { return (0, HOLDOUT); }
                }
                #rest
            }
        }
    };

    quote! {
        #[allow(unused_mut, unused_variables, clippy::needless_return)]
        fn #name(mut head: Head, tape: &mut Tape, mut step_budget: u64) -> (u64, u8) {
            #body
        }
    }
}
