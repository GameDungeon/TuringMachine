use strum::{EnumIter, IntoEnumIterator};

use crate::parser::{self, Action, Program};
use std::fmt;

pub type StateId = usize;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, EnumIter)]
pub enum Sym {
    S0,
    S1,
}

impl std::ops::Not for Sym {
    type Output = Sym;
    fn not(self) -> Sym {
        match self {
            Sym::S0 => Sym::S1,
            Sym::S1 => Sym::S0,
        }
    }
}

/// A straight-line effect on the head.
#[derive(Clone, Copy)]
pub enum Op {
    Flip,
    Write(Sym),
    Shift(Dir),
}

#[derive(Clone, Copy)]
pub enum Term {
    Tail(StateId),
    Halt { state: StateId, symbol: Sym },
}

/// The code for one transition. Always costs exactly one step.
pub struct Block {
    pub ops: Vec<Op>,
    pub term: Term,
}

/// "While reading `scan`, write `write` and move; stay in this state."
/// Consumes a whole run of cells in one go.
#[derive(Clone, Copy)]
pub struct Run {
    pub dir: Dir,
    pub read: Sym,
    pub write: Sym,
}

pub enum Body {
    Straight(Block),
    Switch {
        common: Vec<Op>,
        arms: [Block; 2],
    },
    /// If the cell is `run.scan`, consume the run. Afterwards (or if there was no run)
    /// the cell is known to be `!run.scan`, so we fall straight into `rest`.
    ScanThen {
        run: Run,
        rest: Block,
    },
}

pub struct StateFn {
    pub id: StateId,
    pub body: Body,
}

pub struct Ir {
    pub states: Vec<StateFn>,
}

impl StateFn {
    pub fn blocks(&self) -> Vec<&Block> {
        match &self.body {
            Body::Straight(b) => vec![b],
            Body::Switch { arms: [z, o], .. } => vec![z, o],
            Body::ScanThen { rest, .. } => vec![rest],
        }
    }
}

pub fn lower(p: &Program) -> Ir {
    let states = p
        .states
        .iter()
        .enumerate()
        .map(|(id, arms)| {
            let block = |read: Sym| lower_arm(id, read, arms[read as usize]);

            // A transition back into the same state is a run.
            let fused = Sym::iter().find_map(|s| match arms[s as usize] {
                Action::Go { write, dir, next } if next == id => Some(Run {
                    dir,
                    read: s,
                    write,
                }),
                _ => None,
            });

            let body = match fused {
                Some(run) => Body::ScanThen {
                    run,
                    rest: block(!run.read),
                },
                None => lower_switch(id, arms),
            };
            StateFn { id, body }
        })
        .collect();
    Ir { states }
}

/// Hoist whatever the two arms share into a common prefix.
fn lower_switch(id: StateId, arms: &[Action; 2]) -> Body {
    if let [Action::Go {
        write: write_s0,
        dir: dir_s0,
        next: next_s0,
    }, Action::Go {
        write: write_s1,
        dir: dir_s1,
        next: next_s1,
    }] = *arms
    {
        let write = match (write_s0, write_s1) {
            (Sym::S0, Sym::S0) | (Sym::S1, Sym::S1) => Some(Op::Write(write_s0)), // constant
            (Sym::S1, Sym::S0) => Some(Op::Flip),                                 // write == !read
            (Sym::S0, Sym::S1) => None,                                           // write == read
        };

        if dir_s0 == dir_s1 {
            let mut common: Vec<Op> = write.into_iter().collect();
            common.push(Op::Shift(dir_s0));
            if next_s0 == next_s1 {
                return Body::Straight(Block {
                    ops: common,
                    term: Term::Tail(next_s0),
                });
            }
            let tail = |n| Block {
                ops: vec![],
                term: Term::Tail(n),
            };
            return Body::Switch {
                common,
                arms: [tail(next_s0), tail(next_s1)],
            };
        }
    }
    let block = |read: Sym| lower_arm(id, read, arms[read as usize]);
    Body::Switch {
        common: vec![],
        arms: [block(Sym::S0), block(Sym::S1)],
    }
}

fn lower_arm(id: StateId, read: Sym, action: Action) -> Block {
    match action {
        Action::Halt => Block {
            ops: vec![],
            term: Term::Halt {
                state: id,
                symbol: read,
            },
        },
        Action::Go { write, dir, next } => {
            let mut ops = Vec::new();
            if write != read {
                // No need to write a symbol that matches the tape.
                ops.push(Op::Write(write));
            }
            ops.push(Op::Shift(dir));
            Block {
                ops,
                term: Term::Tail(next),
            }
        }
    }
}

pub fn fmt_ops(ops: &[Op]) -> String {
    ops.iter()
        .map(|op| match op {
            Op::Flip => "flip".to_string(),
            Op::Write(s) => format!("write {}", *s as u8),
            Op::Shift(dir) => format!("shift {dir}"),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

impl fmt::Display for Dir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Left => write!(f, "Left"),
            Self::Right => write!(f, "Right"),
        }
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.ops.is_empty() {
            write!(f, "{}; ", fmt_ops(&self.ops))?;
        }
        match self.term {
            Term::Tail(n) => write!(f, "goto {}", parser::state_name(n)),
            Term::Halt { state, symbol } => {
                write!(f, "halt @{}/{}", parser::state_name(state), symbol as u8)
            }
        }
    }
}

impl fmt::Display for Run {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "run {} over {} writing {}",
            self.dir, self.read as u8, self.write as u8
        )
    }
}

impl quote::ToTokens for Dir {
    fn to_tokens(&self, t: &mut proc_macro2::TokenStream) {
        (*self == Dir::Right).to_tokens(t)
    }
}

impl quote::ToTokens for Sym {
    fn to_tokens(&self, t: &mut proc_macro2::TokenStream) {
        (*self == Sym::S1).to_tokens(t)
    }
}
