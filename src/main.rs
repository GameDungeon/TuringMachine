#![feature(explicit_tail_calls)]
#![expect(incomplete_features)]

use std::fs;
use std::io::Write;
use std::time::Instant;
use std::{fs::OpenOptions, path::Path};

use crate::turing_machine::{Head, Tape};

mod turing_machine;

include!(concat!(env!("OUT_DIR"), "/machine.rs"));

fn main() {
    let mut tape = Tape::new();
    let head = Head::new();

    let start = Instant::now();
    let (steps, state, symbol) = run_machine(head, &mut tape);
    let duration = start.elapsed();
    let time = duration.as_secs_f64();

    let symbol = symbol as u8;

    if !OUTPUT.is_empty() {
        let path = Path::new(OUTPUT);

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).unwrap();
            }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();

        writeln!(file, "{MACHINE} {state} {symbol} {steps}").unwrap();
    }

    println!("{MACHINE} {state} {symbol} {steps}");
    println!("Time: {time}");
}
