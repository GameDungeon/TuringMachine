use crate::ir::{Dir, Sym};

#[derive(Clone, Copy)]
pub enum Action {
    Halt,
    Go { write: Sym, dir: Dir, next: usize },
}

/// `states[i][s]` is what state `i` does when it reads symbol `s`.
pub struct Program {
    pub states: Vec<[Action; 2]>,
}

pub fn parse(src: &str) -> Program {
    let src = src.trim().to_ascii_uppercase();
    let states: Vec<[Action; 2]> = src
        .split('_')
        .map(|g| {
            assert!(
                g.is_ascii() && g.len() == 6,
                "bad state group {g:?}: expected something like 1RB0LC"
            );
            [parse_action(&g[..3]), parse_action(&g[3..])]
        })
        .collect();

    assert!(
        states.len() <= 25,
        "at most 25 states ('Z' is reserved for halt)"
    );
    for (i, arms) in states.iter().enumerate() {
        for a in arms {
            if let Action::Go { next, .. } = a {
                assert!(
                    *next < states.len(),
                    "state {} jumps to undefined state {}",
                    state_name(i),
                    state_name(*next)
                );
            }
        }
    }
    Program { states }
}

fn parse_action(s: &str) -> Action {
    let b = s.as_bytes();
    if b[0] == b'-' {
        return Action::Halt;
    }
    let write = match b[0] {
        b'0' => Sym::S0,
        b'1' => Sym::S1,
        c => panic!("invalid symbol: {}", c as char),
    };
    let dir = match b[1] {
        b'L' => Dir::Left,
        b'R' => Dir::Right,
        c => panic!("invalid direction: {}", c as char),
    };
    match b[2] {
        b'Z' => Action::Halt,
        c @ b'A'..=b'Y' => Action::Go {
            write,
            dir,
            next: (c - b'A') as usize,
        },
        c => panic!("invalid state: {}", c as char),
    }
}

pub fn state_name(id: usize) -> char {
    (b'a' + id as u8) as char
}
