// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! The `Focus` fork-choice strategy — resolve conflicts by φ* (protocol.md).
//!
//! The trustless rule: "when conflicts exist, the particle with higher φ*_i is the
//! canonical choice." Where [`MinHash`](crate::fork::MinHash) gives determinism
//! without attack-resistance, `Focus` makes the winner a function of the whole
//! network's stake-weighted attention — steering it costs controlling the
//! cybergraph's topology, which costs stake.
//!
//! φ* itself is [`tru`]'s, computed by the tri-kernel; foculus never re-derives the
//! kernel. `Focus` builds a [`tru::FocusingGraph`] from the surrounding cyberlinks
//! ([`GraphView`]), computes φ*, and scores each conflicting signal by the focus
//! mass on the particles its links direct attention to. Highest wins; an exact φ*
//! tie falls back to the `content_id` order (the same measure-zero tiebreak the
//! base protocol names), so the outcome is total and deterministic.
//!
//! This is the first, faithful cut. Two refinements are deferred and named where
//! they bite: karma-weighting the graph ([`tru::Context`] is `none` here) and the
//! spec's recompute-φ*-per-candidate ideal (this scores one shared φ* instead).

use tru::{Context, FocusingGraph, FocusingParams, Fx, Link, compute_focusing};

use crate::chain::Signal;
use crate::fork::{ForkChoice, ForkError, GraphView, MinHash};

/// Resolve conflicts by the tri-kernel fixed point φ*.
pub struct Focus {
    params: FocusingParams,
}

impl Focus {
    /// φ* with the default tri-kernel parameters.
    pub fn new() -> Self {
        Self {
            params: FocusingParams::default(),
        }
    }

    /// φ* with explicit tri-kernel parameters (α, λ weights, τ, ε, iter cap).
    pub fn with_params(params: FocusingParams) -> Self {
        Self { params }
    }
}

impl Default for Focus {
    fn default() -> Self {
        Self::new()
    }
}

impl ForkChoice for Focus {
    fn resolve(&self, members: &[Signal], view: &dyn GraphView) -> Result<usize, ForkError> {
        match members.len() {
            0 => return Err(ForkError::Empty),
            1 => return Ok(0),
            _ => {}
        }

        // Build the focus graph from every cyberlink in the surrounding context —
        // stake-weighted edges. (Karma weighting via a non-`none` Context is a
        // named refinement; valence is not yet folded into edge sign.)
        let links: Vec<Link> = view
            .links()
            .iter()
            .map(|l| Link::stake(l.from, l.to, l.amount as u128))
            .collect();

        // No graph to rank on → the deterministic tiebreak, so the outcome is
        // still total. (Also the honest degenerate case: φ* over nothing is nothing.)
        if links.is_empty() {
            return MinHash.resolve(members, view);
        }

        let ctx = Context::none();
        let graph = FocusingGraph::build(links, &ctx);
        let result = compute_focusing(&graph, &self.params);
        let node_ids = graph.node_ids();

        // φ* of a particle: its entry in the focus vector, or zero if the particle
        // is not a node of the graph (a link target no edge reached).
        let focus_of = |p: &[u8; 32]| -> Fx {
            node_ids
                .iter()
                .position(|id| id == p)
                .map(|i| result.focus[i])
                .unwrap_or(Fx::ZERO)
        };

        // Score a signal by the total φ* on the targets its links point at — the
        // attention it directs into the graph.
        let score = |sig: &Signal| -> Fx {
            let mut s = Fx::ZERO;
            for l in &sig.links {
                s = s + focus_of(&l.to);
            }
            s
        };

        // Highest φ* wins; exact tie → lowest content_id (total, deterministic).
        let mut best = 0usize;
        let mut best_score = score(&members[0]);
        let mut best_id = members[0].content_id();
        for (i, m) in members.iter().enumerate().skip(1) {
            let s = score(m);
            let id = m.content_id();
            if s > best_score || (s == best_score && id < best_id) {
                best = i;
                best_score = s;
                best_id = id;
            }
        }
        Ok(best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{CyberlinkRecord, SELF_NETWORK, Signal};
    use crate::fork::LinksView;

    fn p(b: u8) -> [u8; 32] {
        [b; 32]
    }

    fn link(neuron: u8, from: u8, to: u8, amount: u64) -> CyberlinkRecord {
        CyberlinkRecord {
            neuron: p(neuron),
            from: p(from),
            to: p(to),
            token: p(0),
            amount,
            valence: 1,
            height: 0,
        }
    }

    /// A signal whose single link points at `to` — its φ* claim.
    fn sig_to(neuron: u8, step: u64, to: u8) -> Signal {
        Signal {
            neuron: p(neuron),
            network: SELF_NETWORK,
            links: vec![link(neuron, 9, to, 1)],
            delta_pi: vec![],
            box_moves: vec![],
            prev: p(0),
            step,
            height: 0,
            proof: None,
        }
    }

    #[test]
    fn higher_focus_target_wins() {
        // A dense hub around particle 1: many staked edges point at it, so φ*(1)
        // is high. Particle 7 is a lonely fringe node. Two conflicting signals:
        // one directs attention to the hub (1), one to the fringe (7). The hub
        // signal must win — that is φ* fork-choice.
        let context = vec![
            link(2, 2, 1, 1000),
            link(3, 3, 1, 1000),
            link(4, 4, 1, 1000),
            link(5, 5, 1, 1000),
            // the two candidates' own links, so their targets are graph nodes:
            link(1, 9, 1, 1), // candidate A → hub
            link(1, 9, 7, 1), // candidate B → fringe
        ];
        let view = LinksView(context);

        let a = sig_to(1, 0, 1); // targets the hub
        let b = sig_to(1, 0, 7); // targets the fringe
        // canonical member order is by content_id; resolve returns an index into
        // the slice we pass, so pass [a, b] and check which id won.
        let members = vec![a.clone(), b.clone()];
        let idx = Focus::new().resolve(&members, &view).unwrap();
        assert_eq!(
            members[idx].content_id(),
            a.content_id(),
            "the signal directing attention to the high-φ* hub should win"
        );
    }

    #[test]
    fn empty_graph_falls_back_to_minhash() {
        // No links in view and members with no links → φ* has nothing to rank on;
        // the result must still be total and match MinHash exactly.
        let a = Signal {
            neuron: p(1),
            network: SELF_NETWORK,
            links: vec![],
            delta_pi: vec![],
            box_moves: vec![],
            prev: p(0),
            step: 0,
            height: 0,
            proof: None,
        };
        let b = Signal {
            neuron: p(1),
            network: SELF_NETWORK,
            links: vec![],
            delta_pi: vec![(p(5), 1)],
            box_moves: vec![],
            prev: p(0),
            step: 0,
            height: 0,
            proof: None,
        };
        let view = LinksView(vec![]);
        let members = vec![a.clone(), b.clone()];
        let focus_idx = Focus::new().resolve(&members, &view).unwrap();
        let minhash_idx = MinHash.resolve(&members, &view).unwrap();
        assert_eq!(focus_idx, minhash_idx);
    }

    #[test]
    fn deterministic_across_runs() {
        // φ* is fixed-point; two runs must pick the same winner bit-for-bit.
        let view = LinksView(vec![
            link(2, 2, 1, 1000),
            link(1, 9, 1, 1),
            link(1, 9, 7, 1),
        ]);
        let members = vec![sig_to(1, 0, 1), sig_to(1, 0, 7)];
        let r1 = Focus::new().resolve(&members, &view).unwrap();
        let r2 = Focus::new().resolve(&members, &view).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn empty_members_is_error() {
        assert_eq!(
            Focus::new().resolve(&[], &LinksView(vec![])),
            Err(ForkError::Empty)
        );
    }
}
