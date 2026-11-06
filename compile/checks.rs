//! Budget-check placement. Runs last, on the final (inlined and optimized) IR.
//!
//! Every `Block` is a vertex. Its weight is the steps its own ops take (plus the `common`
//! ops when it ends in a branch). Edges go to its arms and to the root block of any node it
//! tail-calls. We choose a minimum set of vertices that hits every cycle and put a check at
//! the start of each. A check at `v` fails when `budget <= chain(v) - 1`, where
//! `chain(v) = weight(v) + max chain(s)` over successors that aren't checked.
use crate::ir::*;
use std::collections::VecDeque;

/// Search nodes the exact solver may expand before falling back to a greedy choice.
const EXACT_BUDGET: u64 = 20_000;

struct Vertex {
    weight: u64,
    succ: Vec<usize>,
}

fn steps_in(ops: &[Op]) -> u64 {
    ops.iter()
        .map(|o| if let Op::Step(n) = o { *n } else { 0 })
        .sum()
}

fn root_block(s: &StateFn) -> &Block {
    match &s.body {
        Body::Code(b) => b,
        // The run path is always cut by the post-run check, so the graph only sees the skip path.
        Body::ScanThen { rest, .. } => rest,
    }
}

/// Pre-order numbering of blocks. `insert` below walks in exactly the same order.
fn collect(b: &Block, verts: &mut Vec<Vertex>, tails: &mut Vec<(usize, NodeId)>) -> usize {
    let id = verts.len();
    verts.push(Vertex {
        weight: steps_in(&b.ops),
        succ: Vec::new(),
    });
    match &b.term {
        Term::Tail(n) => tails.push((id, *n)),
        Term::Halt { .. } => {}
        Term::Branch { arms, .. } => {
            for arm in arms.iter() {
                let k = collect(arm, verts, tails);
                verts[id].succ.push(k);
            }
        }
    }
    id
}

/// Returns the vertices plus each live node's root vertex.
fn build(ir: &Ir, live: &[bool]) -> (Vec<Vertex>, Vec<Option<usize>>) {
    let mut verts = Vec::new();
    let mut tails = Vec::new();
    let mut root = vec![None; ir.states.len()];
    for s in ir.states.iter().filter(|s| live[s.id]) {
        root[s.id] = Some(collect(root_block(s), &mut verts, &mut tails));
    }
    for (v, n) in tails {
        verts[v]
            .succ
            .push(root[n].expect("tail call into a dead node"));
    }
    (verts, root)
}

// ---------------------------------------------------------------- feedback vertex set

/// Shortest cycle among the vertices not in `removed`, if any.
fn shortest_cycle(succ: &[Vec<usize>], removed: &[bool]) -> Option<Vec<usize>> {
    let n = succ.len();
    let mut best: Option<Vec<usize>> = None;
    for start in (0..n).filter(|&v| !removed[v]) {
        let mut parent = vec![usize::MAX; n];
        let mut seen = vec![false; n];
        seen[start] = true;
        let mut queue = VecDeque::from([start]);
        let mut found = None;
        'bfs: while let Some(v) = queue.pop_front() {
            for &w in &succ[v] {
                if removed[w] {
                    continue;
                }
                if w == start {
                    found = Some(v);
                    break 'bfs;
                }
                if !seen[w] {
                    seen[w] = true;
                    parent[w] = v;
                    queue.push_back(w);
                }
            }
        }
        if let Some(mut v) = found {
            let mut cycle = vec![v];
            while v != start {
                v = parent[v];
                cycle.push(v);
            }
            if best.as_ref().map_or(true, |b| cycle.len() < b.len()) {
                best = Some(cycle);
            }
        }
    }
    best
}

/// Can the cycles be broken by removing at most `k` more vertices? Every feedback set must
/// contain a vertex of every cycle, so branching on one shortest cycle is complete.
/// On success `removed` holds the solution.
fn solve(succ: &[Vec<usize>], removed: &mut Vec<bool>, k: usize, nodes_left: &mut u64) -> bool {
    let Some(cycle) = shortest_cycle(succ, removed) else {
        return true;
    };
    if k == 0 || *nodes_left == 0 {
        return false;
    }
    *nodes_left -= 1;
    for v in cycle {
        removed[v] = true;
        if solve(succ, removed, k - 1, nodes_left) {
            return true;
        }
        removed[v] = false;
    }
    false
}

fn feedback_set(succ: &[Vec<usize>]) -> Vec<bool> {
    let n = succ.len();
    let mut budget = EXACT_BUDGET;
    for k in 0..=n {
        let mut removed = vec![false; n];
        if solve(succ, &mut removed, k, &mut budget) {
            return removed;
        }
        if budget == 0 {
            break;
        }
    }
    // Greedy fallback: break the shortest cycle at its best-connected vertex.
    let mut removed = vec![false; n];
    while let Some(cycle) = shortest_cycle(succ, &removed) {
        let pick = *cycle.iter().max_by_key(|&&v| succ[v].len()).unwrap();
        removed[pick] = true;
    }
    removed
}

// ---------------------------------------------------------------- thresholds and insertion

fn chain(v: usize, verts: &[Vertex], checked: &[bool], memo: &mut [Option<u64>]) -> u64 {
    if let Some(c) = memo[v] {
        return c;
    }
    let tail = verts[v]
        .succ
        .iter()
        .filter(|&&s| !checked[s])
        .map(|&s| chain(s, verts, checked, memo))
        .max()
        .unwrap_or(0);
    let c = verts[v].weight + tail;
    memo[v] = Some(c);
    c
}

fn insert(b: &mut Block, next: &mut usize, thresholds: &[Option<u64>]) {
    let id = *next;
    *next += 1;
    if let Some(t) = thresholds[id] {
        b.ops.insert(0, Op::Check(t));
    }
    if let Term::Branch { arms, .. } = &mut b.term {
        for arm in arms.iter_mut() {
            insert(arm, next, thresholds);
        }
    }
}

/// Must run after every pass that adds, removes or moves `Step` ops.
pub fn place(ir: &mut Ir) {
    let live = crate::inline::reachable(ir);
    let (verts, root) = build(ir, &live);
    let n = verts.len();

    let succ: Vec<Vec<usize>> = verts.iter().map(|v| v.succ.clone()).collect();
    let checked = feedback_set(&succ);
    assert!(
        shortest_cycle(&succ, &checked).is_none(),
        "an unchecked cycle survived"
    );

    let mut memo = vec![None; n];
    let chains: Vec<u64> = (0..n)
        .map(|v| chain(v, &verts, &checked, &mut memo))
        .collect();
    let thresholds: Vec<Option<u64>> = (0..n)
        .map(|v| checked[v].then(|| chains[v].saturating_sub(1)))
        .collect();

    // A check that is not at the very start of the entry node needs a guard before it.
    let r0 = root[0].expect("entry node is always live");
    ir.entry_threshold = if checked[r0] {
        0
    } else {
        chains[r0].saturating_sub(1)
    };

    let mut next = 0;
    for s in ir.states.iter_mut().filter(|s| live[s.id]) {
        let r = root[s.id].unwrap();
        debug_assert_eq!(r, next);
        match &mut s.body {
            Body::Code(b) => insert(b, &mut next, &thresholds),
            Body::ScanThen { rest, post, .. } => {
                // Must at least catch an exhausted budget: the run may have stopped because of it.
                *post = if checked[r] {
                    0
                } else {
                    chains[r].saturating_sub(1)
                };
                insert(rest, &mut next, &thresholds);
            }
        }
    }
}
