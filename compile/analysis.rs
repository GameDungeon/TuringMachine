use crate::ir::{Block, Body, Ir, Term};

/// Iterate the set bits of a state bitmask.
fn bits(mask: u32) -> impl Iterator<Item = usize> {
    (0..32usize).filter(move |i| mask >> i & 1 == 1)
}

/// State-to-state control flow. Runs are ops, not edges, so they never appear here;
/// any self edge in this graph is a real, unfused self tail call.
pub struct Graph {
    succ: Vec<Vec<usize>>,
}

impl Graph {
    pub fn of(ir: &Ir) -> Graph {
        let succ = ir
            .states
            .iter()
            .map(|s| {
                let mut v: Vec<usize> = s
                    .blocks()
                    .iter()
                    .filter_map(|b| match b.term {
                        Term::Tail(n) => Some(n),
                        Term::Halt { .. } => None,
                    })
                    .collect();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect();
        Graph { succ }
    }

    fn n(&self) -> usize {
        self.succ.len()
    }

    pub fn reachable(&self, start: usize) -> u32 {
        let mut seen = 1u32 << start;
        let mut stack = vec![start];
        while let Some(v) = stack.pop() {
            for &w in &self.succ[v] {
                if seen >> w & 1 == 0 {
                    seen |= 1 << w;
                    stack.push(w);
                }
            }
        }
        seen
    }

    /// Is the subgraph induced by `nodes` free of cycles (self edges count)?
    pub fn is_acyclic(&self, nodes: u32) -> bool {
        let mut indeg = vec![0usize; self.n()];
        for v in bits(nodes) {
            for &w in &self.succ[v] {
                if nodes >> w & 1 == 1 {
                    indeg[w] += 1;
                }
            }
        }
        let mut stack: Vec<usize> = bits(nodes).filter(|&v| indeg[v] == 0).collect();
        let mut seen = 0;
        while let Some(v) = stack.pop() {
            seen += 1;
            for &w in &self.succ[v] {
                if nodes >> w & 1 == 1 {
                    indeg[w] -= 1;
                    if indeg[w] == 0 {
                        stack.push(w);
                    }
                }
            }
        }
        seen == nodes.count_ones()
    }

    pub fn chains(&self, live: u32, checked: u32) -> Vec<u64> {
        fn go(g: &Graph, v: usize, free: u32, memo: &mut [Option<u64>]) -> u64 {
            if let Some(c) = memo[v] {
                return c;
            }
            let tail = g.succ[v]
                .iter()
                .filter(|&&w| free >> w & 1 == 1)
                .map(|&w| go(g, w, free, memo))
                .max()
                .unwrap_or(0);
            memo[v] = Some(1 + tail);
            1 + tail
        }
        let free = live & !checked;
        let mut memo = vec![None; self.n()];
        for v in bits(live) {
            go(self, v, free, &mut memo);
        }
        memo.into_iter().map(|c| c.unwrap_or(0)).collect()
    }

    /// Smallest set of states whose removal leaves no cycles; ties broken by the
    /// smallest chain bound. This is the one place to change the cost model
    /// (e.g. weight states by a profile).
    pub fn min_feedback_set(&self, live: u32) -> u32 {
        let mut best: Option<(u32, u64, u32)> = None; // (size, chain bound, mask)
        let mut sub = live;
        loop {
            let size = sub.count_ones();
            let worth_trying = best.is_none_or(|(s, ..)| size <= s);
            if worth_trying && self.is_acyclic(live & !sub) {
                let longest = self.chains(live, sub).into_iter().max().unwrap_or(0);
                let key = (size, longest, sub);
                if best.is_none_or(|b| key < b) {
                    best = Some(key);
                }
            }
            if sub == 0 {
                break;
            }
            sub = (sub - 1) & live; // next subset of `live`
        }
        best.expect("the full set is always a feedback set").2
    }

    pub fn inline(&self, ir: &Ir) -> u32 {
        let mut pred_count = vec![0usize; self.n()];
        for state in &self.succ {
            for next in state {
                pred_count[*next] += 1;
            }
        }
        let mut inline = 0;
        for (i, (pred, state)) in pred_count.iter().zip(ir.states.iter()).enumerate().skip(1) {
            if *pred == 1 {
                if let Body::ScanThen { .. } = state.body {
                    continue;
                }

                inline |= 1 << i;
            }
        }
        inline
    }
}

pub struct Analysis {
    /// States reachable from the start state.
    pub reachable: u32,
    /// States that test the budget on entry. Every cycle passes through one.
    pub checked: u32,
    /// States that are only called once and should be inlined.
    pub inline: u32,
    /// Budget threshold: a check fails when `budget <= k`. Also the maximum number of
    /// steps between two checks, so an unchecked chain can never underflow.
    pub chain: Vec<u64>,
}

impl Analysis {
    pub fn is_live(&self, id: usize) -> bool {
        self.reachable >> id & 1 == 1
    }

    pub fn is_checked(&self, id: usize) -> bool {
        self.checked >> id & 1 == 1
    }

    pub fn is_inlined(&self, id: usize) -> bool {
        self.inline >> id & 1 == 1
    }

    /// A state may run only if the budget is above this. At entry (and after a run)
    /// the budget must cover every step up to the next check, so `chain - 1`.
    pub fn threshold(&self, id: usize) -> u64 {
        self.chain[id].saturating_sub(1)
    }
}

pub fn analyze(ir: &Ir) -> Analysis {
    let g = Graph::of(ir);
    let reachable = g.reachable(0);
    let inline = g.inline(ir);
    let checked = g.min_feedback_set(reachable);
    let chain = g.chains(reachable, checked);
    Analysis {
        reachable,
        checked,
        inline, // Need better metric, mixed bag performance
        chain,
    }
}
