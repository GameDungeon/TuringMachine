#![feature(explicit_tail_calls)]
#![expect(incomplete_features)]

use std::time::Instant;

use crate::turing_machine::{Head, Tape};

mod turing_machine;

include!(concat!(env!("OUT_DIR"), "/machine.rs"));

fn main() {
    let start = Instant::now();
    step_for(1_000_000_000);
    let duration = start.elapsed();
    dbg!(duration);
}

pub fn step_for(n: u64) {
    let mut tape = Tape::new();
    let head = Head::new();

    let i = run_machine(head, &mut tape, n);

    println!("Steps: {i}");
}
