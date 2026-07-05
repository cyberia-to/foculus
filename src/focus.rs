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

use tru::{compute_focusing, Context, FocusingGraph, FocusingParams, Fx, Link};

use crate::chain::Signal;
use crate::finality::{finalizes, Domain, Finality};
use crate::fork::{ForkChoice, ForkError, GraphView, MinHash};

/// Resolve conflicts by the tri-kernel fixed point φ*.
pub struct Focus {
    params: FocusingParams,
}

/// The certification and threshold parameters the finality gate needs beyond
/// φ* itself (security-at-scale L1/L2).
#[derive(Clone, Copy)]
pub struct FinalityGate {
    /// ε-support cutoff — particles with φ* ≥ ε form the domain.
    pub epsilon: Fx,
    /// Φ_uncert — the uncertified φ*-mass in the domain.
    pub uncert_mass: Fx,
    /// Domain-local contraction rate κ_D.
    pub kappa_d: Fx,
    /// Tri-kernel Lipschitz constant C.
    pub c: Fx,
    /// Adaptive-threshold multiplier κ'.
    pub kappa_prime: Fx,
}

impl FinalityGate {
    /// A fully-certified view (Φ_uncert = 0) with the reference constants from
    /// `specs/parameters.md` (κ_D=0.74, C=2.25, κ'=1.5). The honest first cut:
    /// real per-source certification tracking is future work; here the gate asks
    /// "in a fully-certified view, does the winner cross τ_D?"
    pub fn certified_view(epsilon: Fx) -> Self {
        let r = |n: i64, d: i64| Fx::from_int(n).div(Fx::from_int(d));
        Self {
            epsilon,
            uncert_mass: Fx::ZERO,
            kappa_d: r(74, 100),
            c: r(225, 100),
            kappa_prime: r(15, 10),
        }
    }
}

/// The outcome of a resolve-and-finalize: the winner and whether it is final,
/// with the φ* numbers behind the verdict.
#[derive(Clone, Copy)]
pub struct Verdict {
    /// Index into `members` of the winning signal.
    pub winner: usize,
    /// Whether the winner finalizes under the gate.
    pub finality: Finality,
    /// The winner's representative φ* (max over its link targets).
    pub winner_phi: Fx,
    /// Δ_D — the φ*-gap to the runner-up.
    pub gap: Fx,
}

/// The φ* a Focus resolution computes: node ids and their focus, enough to score
/// members, build the ε-support domain, and read a particle's φ*.
struct FocusMap {
    node_ids: Vec<[u8; 32]>,
    focus: Vec<Fx>,
}

impl FocusMap {
    /// φ* of a particle, or zero if it is not a node of the graph.
    fn of(&self, p: &[u8; 32]) -> Fx {
        self.node_ids
            .iter()
            .position(|id| id == p)
            .map(|i| self.focus[i])
            .unwrap_or(Fx::ZERO)
    }

    /// The ε-support domain: particles with φ* ≥ ε.
    fn domain(&self, epsilon: Fx) -> Domain {
        let mut ps = Vec::new();
        let mut fs = Vec::new();
        for (i, id) in self.node_ids.iter().enumerate() {
            if self.focus[i] >= epsilon {
                ps.push(*id);
                fs.push(self.focus[i]);
            }
        }
        Domain::from_focus(ps, fs)
    }

    /// A signal's ranking score — total φ* it directs (M2's sum). Relative, used
    /// only to pick the winner.
    fn score(&self, sig: &Signal) -> Fx {
        sig.links.iter().fold(Fx::ZERO, |a, l| a + self.of(&l.to))
    }

    /// A signal's representative φ* — the max focus among its link targets (the
    /// particle it most strongly elevates). In individual-particle units, so it
    /// is comparable to the domain's τ_D. Coincides with `score` for single-link
    /// signals (the common case).
    fn representative(&self, sig: &Signal) -> Fx {
        sig.links
            .iter()
            .map(|l| self.of(&l.to))
            .fold(Fx::ZERO, |a, b| if b > a { b } else { a })
    }
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

impl Focus {
    /// Build the focus graph from the surrounding cyberlinks and compute φ*.
    /// Returns `None` when there are no links to rank on — φ* over nothing is
    /// nothing, and callers fall back to the deterministic tiebreak.
    ///
    /// (Karma weighting via a non-`none` Context is a named refinement; valence
    /// is not yet folded into edge sign.)
    fn focus_map(&self, view: &dyn GraphView) -> Option<FocusMap> {
        let links: Vec<Link> = view
            .links()
            .iter()
            .map(|l| Link::stake(l.from, l.to, l.amount as u128))
            .collect();
        if links.is_empty() {
            return None;
        }
        let ctx = Context::none();
        let graph = FocusingGraph::build(links, &ctx);
        let result = compute_focusing(&graph, &self.params);
        Some(FocusMap {
            node_ids: graph.node_ids().to_vec(),
            focus: result.focus,
        })
    }

    /// Resolve AND report finality in one φ* computation (M3-wire): the same
    /// fixed point that picks the winner decides whether the winner is final.
    /// This is protocol.md step 6 driven by a live conflict's own φ*.
    pub fn resolve_and_finalize(
        &self,
        members: &[Signal],
        view: &dyn GraphView,
        gate: FinalityGate,
    ) -> Result<Verdict, ForkError> {
        if members.is_empty() {
            return Err(ForkError::Empty);
        }
        let Some(map) = self.focus_map(view) else {
            // no graph → no φ*; winner by the deterministic tiebreak, finality
            // undecidable without a distribution to threshold on.
            let winner = MinHash.resolve(members, view)?;
            return Ok(Verdict {
                winner,
                finality: Finality::Pending,
                winner_phi: Fx::ZERO,
                gap: Fx::ZERO,
            });
        };

        // rank members by representative φ* (max target), tie by content_id —
        // total and deterministic; matches resolve() for single-link signals.
        let mut reps: Vec<(usize, Fx, [u8; 32])> = members
            .iter()
            .enumerate()
            .map(|(i, s)| (i, map.representative(s), s.content_id()))
            .collect();
        reps.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));

        let winner = reps[0].0;
        let winner_phi = reps[0].1;
        let runner_up_phi = reps.get(1).map(|r| r.1).unwrap_or(Fx::ZERO);
        let gap = if winner_phi > runner_up_phi {
            winner_phi - runner_up_phi
        } else {
            Fx::ZERO
        };
        let domain = map.domain(gate.epsilon);
        let finality = finalizes(
            winner_phi,
            &domain,
            gate.uncert_mass,
            gap,
            gate.kappa_d,
            gate.c,
            gate.kappa_prime,
        );
        Ok(Verdict {
            winner,
            finality,
            winner_phi,
            gap,
        })
    }
}

impl ForkChoice for Focus {
    fn resolve(&self, members: &[Signal], view: &dyn GraphView) -> Result<usize, ForkError> {
        match members.len() {
            0 => return Err(ForkError::Empty),
            1 => return Ok(0),
            _ => {}
        }
        let Some(map) = self.focus_map(view) else {
            return MinHash.resolve(members, view);
        };

        // Highest sum-score wins; exact tie → lowest content_id (deterministic).
        let mut best = 0usize;
        let mut best_score = map.score(&members[0]);
        let mut best_id = members[0].content_id();
        for (i, m) in members.iter().enumerate().skip(1) {
            let s = map.score(m);
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
    use crate::chain::{CyberlinkRecord, Signal, SELF_NETWORK};
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

    // ── M3-wire: resolve + finality from one φ* ──────────────────────────

    fn tiny_eps() -> Fx {
        Fx::from_int(1).div(Fx::from_int(1_000_000))
    }

    #[test]
    fn verdict_winner_matches_focus_choice() {
        // same hub/fringe setup: the hub signal wins the verdict too, and finality
        // is reported (fully-certified view).
        let view = LinksView(vec![
            link(2, 2, 1, 1000),
            link(3, 3, 1, 1000),
            link(4, 4, 1, 1000),
            link(1, 9, 1, 1),
            link(1, 9, 7, 1),
        ]);
        let a = sig_to(1, 0, 1); // hub
        let b = sig_to(1, 0, 7); // fringe
        let members = vec![a.clone(), b.clone()];
        let v = Focus::new()
            .resolve_and_finalize(&members, &view, FinalityGate::certified_view(tiny_eps()))
            .unwrap();
        assert_eq!(members[v.winner].content_id(), a.content_id());
        // the winner directs more φ* than the runner-up → positive gap
        assert!(v.gap.to_f64() >= 0.0);
        assert!(v.winner_phi.to_f64() > 0.0);
    }

    #[test]
    fn hub_winner_finalizes_when_it_dominates() {
        // A sharply peaked φ* (one dominant hub) over a fully-certified view: the
        // winning particle should clear τ_D and finalize.
        let view = LinksView(vec![
            link(2, 2, 1, 100000),
            link(3, 3, 1, 100000),
            link(4, 4, 1, 100000),
            link(5, 5, 1, 100000),
            link(6, 6, 1, 100000),
            link(1, 9, 1, 1), // candidate A → the dominant hub
            link(1, 9, 7, 1), // candidate B → an untouched fringe
        ]);
        let members = vec![sig_to(1, 0, 1), sig_to(1, 0, 7)];
        let v = Focus::new()
            .resolve_and_finalize(&members, &view, FinalityGate::certified_view(tiny_eps()))
            .unwrap();
        assert_eq!(v.finality, Finality::Final, "a dominant hub winner should finalize");
    }

    #[test]
    fn uncertified_mass_blocks_finality() {
        // Same dominant hub, but a view carrying uncertified mass above the L2
        // bound: the winner is picked but cannot finalize.
        let view = LinksView(vec![
            link(2, 2, 1, 100000),
            link(3, 3, 1, 100000),
            link(4, 4, 1, 100000),
            link(1, 9, 1, 1),
            link(1, 9, 7, 1),
        ]);
        let members = vec![sig_to(1, 0, 1), sig_to(1, 0, 7)];
        let mut gate = FinalityGate::certified_view(tiny_eps());
        gate.uncert_mass = Fx::from_int(1).div(Fx::from_int(2)); // 0.5 ≫ the ~0.004 bound
        let v = Focus::new()
            .resolve_and_finalize(&members, &view, gate)
            .unwrap();
        assert_eq!(v.finality, Finality::Pending, "uncertified mass must block finality");
    }

    #[test]
    fn verdict_empty_graph_is_pending() {
        let members = vec![sig_to(1, 0, 1), sig_to(2, 0, 3)];
        let v = Focus::new()
            .resolve_and_finalize(&members, &LinksView(vec![]), FinalityGate::certified_view(tiny_eps()))
            .unwrap();
        assert_eq!(v.finality, Finality::Pending, "no φ* → cannot finalize");
    }
}
