//! Splice callee bodies into the blocks that tail-call them.
use crate::ir::*;

// Tuning knobs. Benchmark each phase separately.
const STRAIGHT_DEPTH: u32 = 4; // nested splices per path, branch-free callees
const STRAIGHT_MAX: usize = 6; // largest branch-free callee we will copy
const SINGLE_PRED_DEPTH: u32 = 2;
const SINGLE_PRED_MAX: usize = 24;

/// Nodes reachable from the entry (node 0) in the current IR.
pub fn reachable(ir: &Ir) -> Vec<bool> {
    let mut seen = vec![false; ir.states.len()];
    let mut stack = vec![0];
    seen[0] = true;
    while let Some(n) = stack.pop() {
        for t in ir.states[n].successors() {
            if !seen[t] {
                seen[t] = true;
                stack.push(t);
            }
        }
    }
    seen
}

/// Callers per node, counting only code that is still reachable.
pub fn pred_counts(ir: &Ir) -> Vec<usize> {
    let live = reachable(ir);
    let mut preds = vec![0; ir.states.len()];
    preds[0] = 1; // run_machine calls the entry
    for s in ir.states.iter().filter(|s| live[s.id]) {
        for t in s.successors() {
            preds[t] += 1;
        }
    }
    preds
}

struct Cx<'a> {
    callee: &'a [Option<Block>],
    max_depth: u32,
}

impl Cx<'_> {
    fn inline_block(&self, b: &mut Block, mut depth: u32) {
        while let Term::Tail(n) = b.term {
            let Some(callee) = &self.callee[n] else { break };
            if depth >= self.max_depth {
                break;
            }
            b.ops.extend(callee.ops.iter().copied());
            b.term = callee.term.clone();
            depth += 1;
        }
        // The spliced terminator may be a branch: recurse into its arms.
        if let Term::Branch { arms, .. } = &mut b.term {
            for a in arms.iter_mut() {
                self.inline_block(a, depth);
            }
        }
    }
}

fn phase(ir: &mut Ir, max_depth: u32, want: impl Fn(&Block, usize) -> bool) {
    let preds = pred_counts(ir);

    // Snapshot of the bodies we are willing to copy. Never the entry, never a
    // self-recursive node, and never a ScanThen (run loops are not worth duplicating).
    let callee: Vec<Option<Block>> = ir
        .states
        .iter()
        .map(|s| match &s.body {
            Body::Code(b)
                if s.id != 0 && !s.successors().contains(&s.id) && want(b, preds[s.id]) =>
            {
                Some(b.clone())
            }
            _ => None,
        })
        .collect();

    let cx = Cx {
        callee: &callee,
        max_depth,
    };
    for s in &mut ir.states {
        match &mut s.body {
            Body::Code(b) => cx.inline_block(b, 0),
            Body::ScanThen { rest, .. } => cx.inline_block(rest, 0),
        }
    }
}

pub fn run(ir: &mut Ir) {
    // Phase 1: small branch-free nodes. A jump to a two-op function is pure overhead.
    phase(ir, STRAIGHT_DEPTH, |b, _| {
        !matches!(b.term, Term::Branch { .. }) && b.size() <= STRAIGHT_MAX
    });
    // Phase 2: nodes with exactly one caller, counted AFTER phase 1 (it changes the counts).
    phase(ir, SINGLE_PRED_DEPTH, |b, preds| {
        preds == 1 && b.size() <= SINGLE_PRED_MAX
    });
}
