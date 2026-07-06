// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Fold mining — the second lottery that aggregates settlement tickets
//! (`specs/fold-mining.md`).
//!
//! Settlement mining produces a swarm of winning tickets: each is one Shapley
//! marginal `m(n)` under a beacon-seeded ordering. The swarm average converges to
//! the fair division by Hoeffding. But before [[tok]] can mint, those tickets must
//! aggregate into one O(1) object — or a flood of minimum-cost tickets makes
//! settlement verification an asymmetric-cost DoS.
//!
//! This is that aggregation. The accumulated relation for a cluster is the running
//! sum of accepted marginals and the sample count — a commutative monoid under
//! fold, so the tree assembles in any topology without coordination (the leaderless
//! property is preserved) and every path reaches the same accumulator.
//!
//! Two layers:
//!   - value aggregation (here): the `(Σ m(n), k)` monoid over `tru::Fx` marginals,
//!     with the Hoeffding completion target. Field-exact, buildable on what
//!     settlement produces.
//!   - proof aggregation (the interface to `zheng::fold_step`): folding each
//!     ticket's CCS validity proof into one accumulator, so a verifier checks one
//!     O(1) decider per cluster. That needs settlement to emit a per-ticket CCS
//!     proof — its side to produce; this module's monoid is the value half.

use std::collections::BTreeMap;

use tru::Fx;

/// A settlement ticket: one miner's contribution to a cluster's Shapley estimate.
/// `shares` is the running *sum* of accepted marginals (NOT the average) so partial
/// sums from many miners combine cleanly; `samples` is the sample count `k` behind
/// them.
#[derive(Clone, Debug)]
pub struct Ticket {
    pub shares: Vec<([u8; 32], Fx)>,
    pub samples: u64,
}

/// A cluster accumulator: the fold of every ticket. `(Σ m(n), k)` — the running
/// sum of marginals per neuron and the total sample count. Commutative and
/// associative, so tree-assembly order does not affect the result.
#[derive(Clone, Debug, Default)]
pub struct Accumulator {
    shares: BTreeMap<[u8; 32], Fx>,
    samples: u64,
}

impl Accumulator {
    pub fn empty() -> Self {
        Self::default()
    }

    /// The total sample count folded in so far (`k`).
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// Fold one ticket into the accumulator — the monoid operation. Commutative
    /// and associative: folding tickets in any order yields the same accumulator.
    pub fn fold(&mut self, ticket: &Ticket) {
        for (neuron, m) in &ticket.shares {
            let e = self.shares.entry(*neuron).or_insert(Fx::ZERO);
            *e = *e + *m;
        }
        self.samples += ticket.samples;
    }

    /// Fold two accumulators (a tree's internal node). Same monoid; order-free.
    pub fn merge(&mut self, other: &Accumulator) {
        for (neuron, m) in &other.shares {
            let e = self.shares.entry(*neuron).or_insert(Fx::ZERO);
            *e = *e + *m;
        }
        self.samples += other.samples;
    }

    /// The settled Shapley estimate: `Σ m(n) / k` per neuron, in canonical
    /// (neuron id) order. Empty when no samples have been folded.
    pub fn estimate(&self) -> Vec<([u8; 32], Fx)> {
        if self.samples == 0 {
            return Vec::new();
        }
        let k = Fx::from_int(self.samples as i64);
        self.shares
            .iter()
            .map(|(neuron, sum)| (*neuron, sum.div(k)))
            .collect()
    }
}

/// Fold a whole set of tickets into one accumulator — the cluster's fold tree,
/// collapsed. Order-independent by the monoid property, so this equals any
/// pairwise tree assembly.
pub fn fold_all(tickets: &[Ticket]) -> Accumulator {
    let mut acc = Accumulator::empty();
    for t in tickets {
        acc.fold(t);
    }
    acc
}

/// Hoeffding's minimum sample count for a δ-confidence estimate accurate to ±ε:
/// `k_min = ⌈ln(2/δ) / (2ε²)⌉`.
///
/// A protocol-constant derivation done once at the config boundary — not on the
/// per-signal deterministic path — so the `f64` `ln` here is the allowed boundary,
/// same class as the epoch clock. Returns an integer sample count.
pub fn k_min(epsilon: f64, delta: f64) -> u64 {
    ((2.0 / delta).ln() / (2.0 * epsilon * epsilon)).ceil() as u64
}

/// Whether the accumulator has met the Hoeffding precision target — enough samples
/// for a ±ε estimate at δ confidence. Fold work earns nothing beyond this point.
pub fn precision_met(acc: &Accumulator, epsilon: f64, delta: f64) -> bool {
    acc.samples() >= k_min(epsilon, delta)
}

/// What [[tok]] receives for a cluster at the fold deadline (`specs/fold-mining.md`
/// completion and liveness): the settled estimate, and whether it met precision.
/// If too few samples arrived (`k < k_min`), the settled fraction is still applied
/// as the pulse and the remainder defers to the annuity — the chain never stalls.
pub struct Settlement {
    pub estimate: Vec<([u8; 32], Fx)>,
    pub samples: u64,
    pub precision_met: bool,
}

/// Close a cluster at the deadline: whatever has been folded is the settlement,
/// flagged by whether it met the Hoeffding target. A partial (`k < k_min`)
/// accumulator is still valid — its mean over the accumulated subtree satisfies
/// Hoeffding as a sub-sample; tok applies the settled fraction and defers the rest.
pub fn close(acc: &Accumulator, epsilon: f64, delta: f64) -> Settlement {
    Settlement {
        estimate: acc.estimate(),
        samples: acc.samples(),
        precision_met: precision_met(acc, epsilon, delta),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(b: u8) -> [u8; 32] {
        [b; 32]
    }

    fn fx(x: i64, d: i64) -> Fx {
        Fx::from_int(x).div(Fx::from_int(d))
    }

    fn ticket(shares: &[(u8, (i64, i64))], samples: u64) -> Ticket {
        Ticket {
            shares: shares.iter().map(|(a, (x, d))| (n(*a), fx(*x, *d))).collect(),
            samples,
        }
    }

    #[test]
    fn fold_is_commutative() {
        // two tickets, folded in both orders → identical accumulator estimate.
        let a = ticket(&[(1, (30, 100)), (2, (20, 100))], 2);
        let b = ticket(&[(1, (10, 100)), (2, (40, 100))], 2);

        let mut ab = Accumulator::empty();
        ab.fold(&a);
        ab.fold(&b);

        let mut ba = Accumulator::empty();
        ba.fold(&b);
        ba.fold(&a);

        assert_eq!(ab.estimate(), ba.estimate(), "fold order must not matter");
    }

    #[test]
    fn merge_is_associative() {
        let a = ticket(&[(1, (30, 100))], 1);
        let b = ticket(&[(1, (20, 100))], 1);
        let c = ticket(&[(1, (50, 100))], 1);

        // (a ∘ b) ∘ c
        let mut left = fold_all(&[a.clone(), b.clone()]);
        left.merge(&fold_all(&[c.clone()]));

        // a ∘ (b ∘ c)
        let mut right = fold_all(&[a.clone()]);
        right.merge(&fold_all(&[b.clone(), c.clone()]));

        assert_eq!(left.estimate(), right.estimate(), "tree topology must not matter");
    }

    #[test]
    fn estimate_is_the_sample_mean() {
        // one neuron, running sum 1.0 over 4 samples → mean 0.25.
        let t = ticket(&[(1, (100, 100))], 4);
        let acc = fold_all(&[t]);
        let est = acc.estimate();
        assert_eq!(est.len(), 1);
        assert!((est[0].1.to_f64() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn empty_accumulator_estimates_nothing() {
        assert!(Accumulator::empty().estimate().is_empty());
    }

    #[test]
    fn hoeffding_k_min_matches_the_spec_number() {
        // ε = 1%, δ = 1e-6 → k_min = ⌈ln(2e6)/(2·1e-4)⌉ ≈ 72,548 (fold-mining.md)
        let k = k_min(0.01, 1e-6);
        assert!((72_000..73_000).contains(&k), "got {k}");
    }

    #[test]
    fn precision_gates_on_k_min() {
        let mut acc = Accumulator::empty();
        // fold one ticket short of the target, then over it
        acc.fold(&ticket(&[(1, (10, 100))], 100));
        assert!(!precision_met(&acc, 0.01, 1e-6));
        acc.fold(&ticket(&[(1, (10, 100))], 80_000));
        assert!(precision_met(&acc, 0.01, 1e-6));
    }

    #[test]
    fn close_flags_partial_settlements() {
        // too few samples → still a valid settlement, flagged not-precise; the
        // chain does not stall (tok applies the settled fraction).
        let acc = fold_all(&[ticket(&[(1, (50, 100))], 10)]);
        let s = close(&acc, 0.01, 1e-6);
        assert_eq!(s.samples, 10);
        assert!(!s.precision_met);
        assert_eq!(s.estimate.len(), 1);
    }
}
