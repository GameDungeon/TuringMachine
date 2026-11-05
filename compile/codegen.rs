use std::env;

use crate::analysis::Analysis;
use crate::ir::*;
use crate::parser::state_name;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

fn fn_ident(id: StateId) -> Ident {
    format_ident!("state_{}", state_name(id))
}

/// Halt code = state * 2 + symbol. `Z` (u8::MAX) means "out of budget".
fn halt_code(state: StateId, symbol: Sym) -> u8 {
    (state * 2 + symbol as usize) as u8
}

pub fn emit(ir: &Ir, an: &Analysis, machine: &str) -> TokenStream {
    let k = an.threshold(0);
    let names: Vec<String> = (0..ir.states.len())
        .map(|i| state_name(i).to_string().to_uppercase())
        .collect();
    let n = names.len();
    let fns: Vec<TokenStream> = ir
        .states
        .iter()
        .filter(|s| an.is_live(s.id))
        .map(|s| emit_state(s, an))
        .collect();

    let steps = env::var_os("STEPS")
        .map(|x| x.to_str().unwrap().parse().unwrap())
        .unwrap_or(10u64.pow(10))
        + an.chain.iter().max().unwrap();

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
            // Establishes the "budget > k" invariant for a start state that is unchecked.
            if STEPS <= #k {
                return (0, "HOLDOUT", false);
            }

            let (left, code) = state_a(head, tape, STEPS);

            if code == HOLDOUT {
                (0, "HOLDOUT", false)
            } else {
                (STEPS - left, NAMES[(code >> 1) as usize], code & 1 != 0)
            }
        }

        #(#fns)*
    }
}

fn emit_state(s: &StateFn, an: &Analysis) -> TokenStream {
    let k = an.threshold(s.id);
    let name = fn_ident(s.id);

    let inline = if an.is_inlined(s.id) {
        quote! {#[inline(always)]}
    } else {
        quote! {}
    };

    let entry_check = if an.is_checked(s.id) {
        quote! {
           if step_budget <= #k {
               return (0, HOLDOUT);
           }
        }
    } else {
        quote! {}
    };

    let (mut_budget, body) = match &s.body {
        Body::Straight(b) => (quote! {}, emit_block(b)),
        Body::Switch {
            common,
            arms: [zero, one],
        } => {
            let (zero, one) = (emit_block(zero), emit_block(one));
            if common.is_empty() {
                (quote! {}, quote! { if head.read() { #one } else { #zero } })
            } else {
                let ops = common.iter().map(emit_op);
                // Read BEFORE the hoisted ops: they rewrite or leave the cell.
                (
                    quote! {},
                    quote! {
                        let sym = head.read();
                        #(#ops)*
                        if sym { #one } else { #zero }
                    },
                )
            }
        }
        Body::ScanThen { run, rest } => {
            let Run { dir, read, write } = *run;
            let cond = match read {
                Sym::S0 => quote! { !head.read() },
                Sym::S1 => quote! { head.read() },
            };
            let rest = emit_block(rest);
            (
                quote! { mut },
                quote! {
                    if #cond {
                        step_budget = head.shift_rule::<#dir, #read, #write>(tape, step_budget);
                        // let n = head.shift_rule::<#dir, #read, #write>(tape, step_budget);
                        // step_budget -= n;
                        if step_budget <= #k {
                            return (0, HOLDOUT);
                        }
                    }
                    #rest
                },
            )
        }
    };

    quote! {
        #inline
        #[allow(unused_mut, unused_variables, clippy::needless_return)]
        fn #name(mut head: Head, tape: &mut Tape, #mut_budget step_budget: u64) -> (u64, u8) {
            #entry_check
            #body
        }
    }
}

fn emit_block(b: &Block) -> TokenStream {
    let ops = b.ops.iter().map(emit_op);
    let term = match b.term {
        Term::Tail(next) => {
            let f = fn_ident(next);
            quote! { become #f(head, tape, step_budget - 1) }
        }
        Term::Halt { state, symbol } => {
            let code = halt_code(state, symbol);
            quote! {
                return (step_budget - 1, #code)
            }
        }
    };
    quote! { #(#ops)* #term }
}

fn emit_op(op: &Op) -> TokenStream {
    match *op {
        Op::Flip => quote! { head.flip(); },
        Op::Write(s) => quote! { head.write::<#s>(); },
        Op::Shift(dir) => quote! { head.shift::<#dir>(tape); },
    }
}
