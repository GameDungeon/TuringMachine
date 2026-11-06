//! Small IR cleanups that run after inlining. Each one keeps step counts exactly as they were.
use crate::ir::*;

pub fn optimize_ir(ir: &mut Ir) {
    trim_dead_head_ops(ir);
    merge_steps(ir);
}

fn trim_dead_head_ops(ir: &mut Ir) {
    for_each_block(ir, trim_block);
}

fn kill_head_effects(ops: &mut Vec<Op>) {
    ops.retain(|op| matches!(op, Op::Step(_)));
}

fn trim_block(b: &mut Block) {
    match &mut b.term {
        Term::Tail(_) => {}
        // Nothing runs after a halt, so no head effect in this block can be observed.
        Term::Halt { .. } => kill_head_effects(&mut b.ops),
        Term::Branch { arms } => {
            for arm in arms.iter_mut() {
                trim_block(arm);
            }
        }
    }
}

fn merge_ops(ops: &mut Vec<Op>) {
    let mut total = 0u64;
    let mut out = Vec::with_capacity(ops.len());
    for op in ops.drain(..) {
        match op {
            Op::Step(n) => total += n,
            other => out.push(other),
        }
    }
    if total > 0 {
        out.push(Op::Step(total));
    }
    *ops = out;
}

fn merge_block(b: &mut Block) {
    merge_ops(&mut b.ops);
    if let Term::Branch { arms } = &mut b.term {
        for arm in arms.iter_mut() {
            merge_block(arm);
        }
    }
}

fn merge_steps(ir: &mut Ir) {
    for_each_block(ir, merge_block);
}

fn for_each_block(ir: &mut Ir, f: fn(&mut Block)) {
    for s in &mut ir.states {
        match &mut s.body {
            Body::Code(b) => f(b),
            Body::ScanThen { rest, .. } => f(rest),
        }
    }
}
