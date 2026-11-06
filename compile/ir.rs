use strum::{EnumIter, IntoEnumIterator};

use crate::parser::{self, Action, Program};
use std::collections::HashMap;

pub type StateId = usize;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, EnumIter, Hash)]
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
    Step(u64),
    Check(u64),
}

#[derive(Clone)]
pub enum Term {
    Tail(NodeId),
    Halt { state: StateId, symbol: Sym },
    Branch { arms: Box<[Block; 2]> },
}

#[derive(Clone)]
pub struct Block {
    pub ops: Vec<Op>,
    pub term: Term,
}

pub enum Body {
    Code(Block),
    ScanThen { run: Run, rest: Block, post: u64 },
}

impl Block {
    /// Every node this block can tail-call, including inside nested branches.
    pub fn tails(&self, out: &mut Vec<NodeId>) {
        match &self.term {
            Term::Tail(n) => out.push(*n),
            Term::Halt { .. } => {}
            Term::Branch { arms, .. } => {
                arms[0].tails(out);
                arms[1].tails(out);
            }
        }
    }

    /// Rough code size, used for inlining limits.
    pub fn size(&self) -> usize {
        self.ops.len()
            + match &self.term {
                Term::Branch { arms } => 1 + arms[0].size() + arms[1].size(),
                _ => 1,
            }
    }
}

/// "While reading `scan`, write `write` and move; stay in this state."
/// Consumes a whole run of cells in one go.
#[derive(Clone, Copy)]
pub struct Run {
    pub dir: Dir,
    pub read: Sym,
    pub write: Sym,
}

/// Index of a (state, context) pair. Each node becomes one generated function.
pub type NodeId = usize;

/// What we statically know about the cells `[left, here, right]` around the head.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Ctx {
    pub win: [Option<Sym>; 3],
}

impl Ctx {
    pub fn here(self) -> Option<Sym> {
        self.win[1]
    }

    /// Knowledge after writing `write` at the head and moving one cell.
    pub fn after(self, write: Sym, dir: Dir) -> Ctx {
        let [l, _, r] = self.win;
        Ctx {
            win: match dir {
                Dir::Right => [Some(write), r, None],
                Dir::Left => [None, l, Some(write)],
            },
        }
    }

    /// Keep only what both contexts agree on.
    pub fn meet(self, other: Ctx) -> Ctx {
        let mut win = self.win;
        for (w, o) in win.iter_mut().zip(other.win) {
            if *w != o {
                *w = None;
            }
        }
        Ctx { win }
    }

    /// Function-name suffix: empty for the default context, e.g. `_x1x` otherwise.
    pub fn suffix(self) -> String {
        if self == Ctx::default() {
            return String::new();
        }
        let cells: String = self
            .win
            .iter()
            .map(|c| match c {
                None => 'x',
                Some(Sym::S0) => '0',
                Some(Sym::S1) => '1',
            })
            .collect();
        format!("_{cells}")
    }
}

pub struct Ir {
    /// Indexed by node id. Node 0 is the start.
    pub states: Vec<StateFn>,
    /// Number of states in the source program (not nodes).
    pub n_states: usize,
    /// `run_machine` refuses to start if the budget is <= this. Set by `checks::place`.
    pub entry_threshold: u64,
}

pub struct StateFn {
    pub id: NodeId,
    pub state: StateId,
    pub ctx: Ctx,
    pub body: Body,
}

impl StateFn {
    pub fn successors(&self) -> Vec<NodeId> {
        let mut v = Vec::new();
        match &self.body {
            Body::Code(b) => b.tails(&mut v),
            Body::ScanThen { rest, .. } => rest.tails(&mut v),
        }
        v
    }
}

fn fused_run(arms: &[Action; 2], id: StateId) -> Option<Run> {
    // A transition back into the same state is a run.
    Sym::iter().find_map(|s| match arms[s as usize] {
        Action::Go { write, dir, next } if next == id => Some(Run {
            dir,
            read: s,
            write,
        }),
        _ => None,
    })
}

struct Lowering<'a> {
    p: &'a Program,
    ids: HashMap<(StateId, Ctx), NodeId>,
    keys: Vec<(StateId, Ctx)>,
}

impl<'a> Lowering<'a> {
    fn node(&mut self, state: StateId, ctx: Ctx) -> NodeId {
        // Run states keep no context: it would only duplicate the run loop.
        let ctx = if fused_run(&self.p.states[state], state).is_some() {
            Ctx::default()
        } else {
            ctx
        };
        if let Some(&id) = self.ids.get(&(state, ctx)) {
            return id;
        }
        let id = self.keys.len();
        self.keys.push((state, ctx));
        self.ids.insert((state, ctx), id);
        id
    }

    fn lower_node(&mut self, state: StateId, ctx: Ctx) -> Body {
        let p = self.p;
        let arms = &p.states[state];

        if let Some(run) = fused_run(arms, state) {
            // After the run, or if we skipped it, the cell is !run.read. Nothing else is
            // known on both paths, so the successor gets an empty context.
            let rest = self.lower_arm(state, !run.read, arms[(!run.read) as usize], Ctx::default());
            return Body::ScanThen { run, rest, post: 0 };
        }
        if let Some(here) = ctx.here() {
            // The read is statically resolved.
            return Body::Code(self.lower_arm(state, here, arms[here as usize], ctx));
        }
        Body::Code(self.lower_switch(state, arms, ctx))
    }

    /// Hoist whatever the two arms share into a common prefix.
    fn lower_switch(&mut self, id: StateId, arms: &[Action; 2], ctx: Ctx) -> Block {
        let b0 = self.lower_arm(id, Sym::S0, arms[0], ctx);
        let b1 = self.lower_arm(id, Sym::S1, arms[1], ctx);
        Block {
            ops: vec![],
            term: Term::Branch {
                arms: Box::new([b0, b1]),
            },
        }
    }

    fn lower_arm(&mut self, id: StateId, read: Sym, action: Action, ctx: Ctx) -> Block {
        match action {
            // The halting transition still costs a step.
            Action::Halt => Block {
                ops: vec![Op::Step(1)],
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
                ops.push(Op::Step(1));
                let to = self.node(next, ctx.after(write, dir));
                Block {
                    ops,
                    term: Term::Tail(to),
                }
            }
        }
    }
}

pub fn lower(p: &Program) -> Ir {
    let mut lw = Lowering {
        p,
        ids: HashMap::new(),
        keys: Vec::new(),
    };
    lw.node(0, Ctx::default());

    // Worklist: lowering a node may intern new ones, which are appended and processed later.
    let mut states = Vec::new();
    let mut i = 0;
    while i < lw.keys.len() {
        let (state, ctx) = lw.keys[i];
        let body = lw.lower_node(state, ctx);
        states.push(StateFn {
            id: i,
            state,
            ctx,
            body,
        });
        i += 1;
    }
    Ir {
        states,
        n_states: p.states.len(),
        entry_threshold: 0,
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
