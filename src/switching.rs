// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Support switching — the honest-split resolver (protocol.md step 4, T1).
//!
//! Contraction converges φ* *toward* a fixed point; it does not move stake between
//! two competing ones. So a near-50/50 honest split does not resolve on its own —
//! the gap Δ_D is a static property of a static support set. This is the rule that
//! moves it: each round, a neuron with stake on the locally-losing member of a
//! conflict re-points to the local leader with probability `q`. The losing side's
//! support migrates until the gap crosses the finality threshold.
//!
//! This models the *deterministic drift* of T1 — once a leader exists, honest
//! re-pointing amplifies its lead geometrically. The stochastic escape from an
//! exact tie (T1's anti-concentration step) is separate, named proof debt in
//! `roadmap/honest-split-anti-concentration.md`; here `leader()` breaks an exact
//! tie deterministically by index so the drift always has a direction.
//!
//! Switching signals are consensus-only — zero mint, zero BTS exposure — so
//! finality timing cannot be gamed for reward (the economic-neutrality invariant
//! from `specs/protocol.md`). This module is stake dynamics only; the signal
//! emission and its VDF rate limit live at the protocol layer.
//!
//! Everything is `tru::Fx` fixed-point — no float on the deterministic path.

use tru::Fx;

/// Per-member support in a conflict: the stake pointing at each competing member,
/// normalized to sum to 1.
#[derive(Clone, Debug)]
pub struct Support {
    weights: Vec<Fx>,
}

impl Support {
    /// Normalize a raw stake vector to the simplex. An all-zero (or empty) input
    /// yields a uniform distribution — no member is favored.
    pub fn new(raw: Vec<Fx>) -> Self {
        if raw.is_empty() {
            return Self { weights: raw };
        }
        let total = raw.iter().fold(Fx::ZERO, |a, w| a + *w);
        let weights = if total > Fx::ZERO {
            raw.iter().map(|w| w.div(total)).collect()
        } else {
            let u = Fx::ONE.div(Fx::from_int(raw.len() as i64));
            vec![u; raw.len()]
        };
        Self { weights }
    }

    pub fn len(&self) -> usize {
        self.weights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.weights.is_empty()
    }

    pub fn weights(&self) -> &[Fx] {
        &self.weights
    }

    /// The current leader — the member with the most support. An exact tie is
    /// broken by lowest index, so the drift always has a direction (this is the
    /// deterministic stand-in for T1's anti-concentration escape).
    pub fn leader(&self) -> usize {
        let mut best = 0usize;
        for i in 1..self.weights.len() {
            if self.weights[i] > self.weights[best] {
                best = i;
            }
        }
        best
    }

    /// Δ — the gap between the leader and the runner-up. Zero for a single member.
    pub fn gap(&self) -> Fx {
        if self.weights.len() < 2 {
            return Fx::ZERO;
        }
        let l = self.leader();
        let leader_w = self.weights[l];
        let mut runner_up = Fx::ZERO;
        for (i, w) in self.weights.iter().enumerate() {
            if i != l && *w > runner_up {
                runner_up = *w;
            }
        }
        if leader_w > runner_up {
            leader_w - runner_up
        } else {
            Fx::ZERO
        }
    }

    /// One switching round: every non-leader member sheds a `q` fraction of its
    /// support to the current leader (honest losing-side re-pointing). Support is
    /// conserved — the leader gains exactly what the others give up.
    pub fn switch_round(&mut self, q: Fx) {
        if self.weights.len() < 2 {
            return;
        }
        let l = self.leader();
        let mut moved = Fx::ZERO;
        for (i, w) in self.weights.iter_mut().enumerate() {
            if i != l {
                let shed = *w * q;
                *w = *w - shed;
                moved = moved + shed;
            }
        }
        self.weights[l] = self.weights[l] + moved;
    }
}

/// Rounds until the leader's gap exceeds `threshold`, capped at `max_rounds`.
/// `None` if the cap is hit first (a symmetric adversarial hold, or a threshold
/// at/above 1). Does not mutate the input.
pub fn rounds_to_resolve(
    support: &Support,
    q: Fx,
    threshold: Fx,
    max_rounds: usize,
) -> Option<usize> {
    let mut s = support.clone();
    for round in 0..max_rounds {
        if s.gap() > threshold {
            return Some(round);
        }
        s.switch_round(q);
    }
    if s.gap() > threshold {
        Some(max_rounds)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(n: i64, d: i64) -> Fx {
        Fx::from_int(n).div(Fx::from_int(d))
    }

    fn sum(s: &Support) -> f64 {
        s.weights().iter().map(|w| w.to_f64()).sum()
    }

    #[test]
    fn normalizes_to_the_simplex() {
        let s = Support::new(vec![fx(3, 1), fx(1, 1)]); // 3:1 → 0.75, 0.25
        assert!((s.weights()[0].to_f64() - 0.75).abs() < 1e-6);
        assert!((sum(&s) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn all_zero_is_uniform() {
        let s = Support::new(vec![Fx::ZERO, Fx::ZERO, Fx::ZERO]);
        for w in s.weights() {
            assert!((w.to_f64() - 1.0 / 3.0).abs() < 1e-6);
        }
    }

    #[test]
    fn switching_conserves_support() {
        let mut s = Support::new(vec![fx(51, 100), fx(49, 100)]);
        for _ in 0..10 {
            s.switch_round(fx(1, 5));
            assert!((sum(&s) - 1.0).abs() < 1e-6, "support must stay on the simplex");
        }
    }

    #[test]
    fn switching_amplifies_the_leader() {
        let mut s = Support::new(vec![fx(52, 100), fx(48, 100)]);
        let g0 = s.gap().to_f64();
        s.switch_round(fx(1, 4));
        let g1 = s.gap().to_f64();
        assert!(g1 > g0, "one round should widen the gap");
    }

    #[test]
    fn a_near_tie_resolves_in_finite_rounds() {
        let s = Support::new(vec![fx(51, 100), fx(49, 100)]);
        // gap must exceed 0.9 (leader ≈ 0.95): geometric, so a modest round count
        let rounds = rounds_to_resolve(&s, fx(1, 4), fx(9, 10), 100);
        assert!(rounds.is_some(), "an honest majority must eventually resolve");
    }

    #[test]
    fn exact_tie_still_resolves_by_index_tiebreak() {
        // 50/50 — leader() breaks the tie by index, so the drift has a direction.
        let s = Support::new(vec![fx(1, 2), fx(1, 2)]);
        let rounds = rounds_to_resolve(&s, fx(1, 4), fx(9, 10), 100);
        assert!(rounds.is_some(), "even an exact tie resolves via the deterministic tiebreak");
    }

    #[test]
    fn smaller_q_takes_more_rounds() {
        let s = Support::new(vec![fx(55, 100), fx(45, 100)]);
        let fast = rounds_to_resolve(&s, fx(1, 2), fx(9, 10), 500).unwrap();
        let slow = rounds_to_resolve(&s, fx(1, 10), fx(9, 10), 500).unwrap();
        assert!(slow > fast, "a lower switching rate must take more rounds");
    }

    #[test]
    fn three_way_split_resolves_to_the_plurality() {
        let s = Support::new(vec![fx(40, 100), fx(35, 100), fx(25, 100)]);
        assert_eq!(s.leader(), 0);
        let rounds = rounds_to_resolve(&s, fx(1, 4), fx(9, 10), 200);
        assert!(rounds.is_some());
    }
}
