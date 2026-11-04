#![feature(explicit_tail_calls)]
#![expect(incomplete_features)]

// use std::time::Instant;

use std::fs::OpenOptions;
use std::io::Write;

use crate::turing_machine::{Head, Tape};

mod turing_machine;

include!(concat!(env!("OUT_DIR"), "/machine.rs"));

fn main() {
    // let start = Instant::now();
    step_for(100_000_000_000);
    // let duration = start.elapsed();
    // dbg!(duration);
}

pub fn step_for(n: u64) {
    let mut tape = Tape::new();
    let head = Head::new();

    let (steps, state, symbol) = run_machine(head, &mut tape, n);

    let state = state.to_ascii_uppercase();
    let symbol = symbol as u8;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("machine_output.txt")
        .unwrap();

    // println!("{MACHINE} {steps} {state} {symbol}");
    writeln!(file, "{MACHINE} {steps} {state} {symbol}").unwrap();
}
