// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Domain-local finality — protocol.md step 6 (security-at-scale L1/L2).
//!
//! A particle finalizes in a neuron's view iff two local reads both hold:
//!   1. adaptive threshold — φ*_i > τ_D = μ_D + κ'·σ_D, over the particle's own
//!      ε-support domain D (not the planetary graph).
//!   2. certification gate — the uncertified φ*-mass in D is below the L2 bound,
//!      so no hidden source could flip the ranking:
//!      Φ_uncert < (1−κ_D)·Δ_D / (C·(1+κ')).
//!
//! Both are computed in `tru`'s fixed-point field arithmetic — no float on the
//! deterministic path. The one subtlety: σ_D needs a square root the Goldilocks
//! field cannot take. The threshold is instead tested in its squared, sqrt-free
//! form: for an above-mean particle, φ*_i > μ + κ'σ ⟺ (φ*_i − μ)² > κ'²·var_D.
//! Exact in the field, and equivalent to the spec's condition.

use tru::{Fx, FocusingResult};

use bbg::Particle;

/// An ε-support domain: the particles whose φ* is significant, and their focus.
/// This is the canonical, content-defined region reward-spec §7 settlement also
/// uses — the same object safety and liveness are established over.
pub struct Domain {
    particles: Vec<Particle>,
    focus: Vec<Fx>, // parallel to `particles`
}

impl Domain {
    /// The ε-support: the superlevel set of the focus distribution — every particle
    /// with φ* ≥ ε. Canonical and deterministic given the graph result and ε.
    pub fn epsilon_support(result: &FocusingResult, node_ids: &[Particle], epsilon: Fx) -> Domain {
        let mut particles = Vec::new();
        let mut focus = Vec::new();
        for (i, id) in node_ids.iter().enumerate() {
            if result.focus[i] >= epsilon {
                particles.push(*id);
                focus.push(result.focus[i]);
            }
        }
        Domain { particles, focus }
    }

    /// Construct directly from parallel (particle, φ*) vectors — used by callers
    /// that already hold a domain's focus, and by tests.
    pub fn from_focus(particles: Vec<Particle>, focus: Vec<Fx>) -> Domain {
        debug_assert_eq!(particles.len(), focus.len());
        Domain { particles, focus }
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// μ_D — the mean φ* over the domain.
    pub fn mean(&self) -> Fx {
        if self.focus.is_empty() {
            return Fx::ZERO;
        }
        let mut sum = Fx::ZERO;
        for f in &self.focus {
            sum = sum + *f;
        }
        sum.div(Fx::from_int(self.focus.len() as i64))
    }

    /// var_D = E[φ²] − μ², clamped at zero.
    ///
    /// The clamp is not cosmetic: in fixed-point, rounding can make E[φ²] a hair
    /// below μ², and field subtraction of a < b wraps to p − (b − a) — a huge
    /// value that would wreck the threshold test. Variance is non-negative, so any
    /// such wrap is exactly the case to floor at zero.
    pub fn variance(&self) -> Fx {
        if self.focus.is_empty() {
            return Fx::ZERO;
        }
        let mu = self.mean();
        let mut sq = Fx::ZERO;
        for f in &self.focus {
            sq = sq + (*f * *f);
        }
        let mean_sq = sq.div(Fx::from_int(self.focus.len() as i64));
        let mu_sq = mu * mu;
        if mean_sq >= mu_sq {
            mean_sq - mu_sq
        } else {
            Fx::ZERO
        }
    }

    /// φ* of a particle in this domain, if it is in the ε-support.
    pub fn focus_of(&self, particle: &Particle) -> Option<Fx> {
        self.particles
            .iter()
            .position(|p| p == particle)
            .map(|i| self.focus[i])
    }
}

/// The adaptive-threshold test, sqrt-free: φ*_i > μ_D + κ'·σ_D.
///
/// For an above-mean particle this is equivalent to (φ*_i − μ)² > κ'²·var_D, which
/// avoids the square root σ_D would need. A particle at or below the mean can never
/// clear μ + κ'σ (κ' ≥ 1, σ ≥ 0), so it is rejected outright.
pub fn crosses_threshold(phi_i: Fx, domain: &Domain, kappa_prime: Fx) -> bool {
    let mu = domain.mean();
    if phi_i <= mu {
        return false;
    }
    let excess = phi_i - mu;
    let lhs = excess * excess; // (φ*_i − μ)²
    let rhs = (kappa_prime * kappa_prime) * domain.variance(); // κ'²·var_D
    lhs > rhs
}

/// The L2 certification gate: uncertified φ*-mass must be below the safety bound,
/// so no source still holding out could hide enough weight to flip the winner.
///
/// Φ_uncert < (1−κ_D)·Δ_D / (C·(1+κ')). Tested by cross-multiplication
/// (Φ_uncert·denom < numer), which is exact and avoids the division; the
/// denominator C·(1+κ') is strictly positive, so the inequality direction is
/// preserved. Precondition: κ_D < 1 (always true for a contraction rate).
pub fn certified(uncert_mass: Fx, gap: Fx, kappa_d: Fx, c: Fx, kappa_prime: Fx) -> bool {
    let one = Fx::ONE;
    let numer = (one - kappa_d) * gap; // (1−κ_D)·Δ_D
    let denom = c * (one + kappa_prime); // C·(1+κ')
    uncert_mass * denom < numer
}

/// A particle's finality status in a neuron's view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finality {
    /// Both conditions hold — final, irreversible.
    Final,
    /// At least one condition fails — still pending.
    Pending,
}

/// protocol.md step 6: a particle finalizes iff it crosses the adaptive threshold
/// AND the certification gate holds. Both are local reads; neither sends a message.
#[allow(clippy::too_many_arguments)]
pub fn finalizes(
    phi_i: Fx,
    domain: &Domain,
    uncert_mass: Fx,
    gap: Fx,
    kappa_d: Fx,
    c: Fx,
    kappa_prime: Fx,
) -> Finality {
    if crosses_threshold(phi_i, domain, kappa_prime)
        && certified(uncert_mass, gap, kappa_d, c, kappa_prime)
    {
        Finality::Final
    } else {
        Finality::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(b: u8) -> Particle {
        [b; 32]
    }

    fn fx(n: i64, d: i64) -> Fx {
        Fx::from_int(n).div(Fx::from_int(d))
    }

    #[test]
    fn epsilon_support_is_the_superlevel_set() {
        let result = FocusingResult {
            focus: vec![fx(1, 2), fx(1, 100), fx(3, 10)],
            syntropy: Fx::ZERO,
            diffusion: vec![],
            springs: vec![],
            heat: vec![],
        };
        let ids = vec![p(1), p(2), p(3)];
        // ε = 1/20: keeps 1/2 and 3/10, drops 1/100
        let d = Domain::epsilon_support(&result, &ids, fx(1, 20));
        assert_eq!(d.len(), 2);
        assert!(d.focus_of(&p(1)).is_some());
        assert!(d.focus_of(&p(2)).is_none());
        assert!(d.focus_of(&p(3)).is_some());
    }

    #[test]
    fn mean_and_variance_match_hand_computation() {
        // φ = [0.25, 0.25, 0.5]; μ = 1/3; E[φ²] = (0.0625+0.0625+0.25)/3 = 0.125;
        // var = 0.125 − 1/9 ≈ 0.01389
        let d = Domain::from_focus(vec![p(1), p(2), p(3)], vec![fx(1, 4), fx(1, 4), fx(1, 2)]);
        assert!((d.mean().to_f64() - 1.0 / 3.0).abs() < 1e-6);
        assert!((d.variance().to_f64() - 0.013888).abs() < 1e-4);
    }

    #[test]
    fn uniform_domain_has_zero_variance_and_nothing_crosses() {
        // all equal → var = 0, μ = the common value; no particle is strictly above μ
        let d = Domain::from_focus(vec![p(1), p(2), p(3)], vec![fx(1, 3), fx(1, 3), fx(1, 3)]);
        assert!(d.variance().to_f64() < 1e-9);
        assert!(!crosses_threshold(fx(1, 3), &d, fx(3, 2)));
    }

    #[test]
    fn a_spike_crosses_the_threshold() {
        // one dominant particle over a flat low background clears μ + κ'σ
        let d = Domain::from_focus(
            vec![p(1), p(2), p(3), p(4)],
            vec![fx(85, 100), fx(5, 100), fx(5, 100), fx(5, 100)],
        );
        assert!(crosses_threshold(fx(85, 100), &d, fx(3, 2)), "the spike should finalize");
        assert!(!crosses_threshold(fx(5, 100), &d, fx(3, 2)), "background should not");
    }

    #[test]
    fn certification_gate_bounds_uncertified_mass() {
        // bound = (1−κ_D)·Δ_D / (C·(1+κ')); with κ_D=0.74, Δ_D=0.085, C=2.25, κ'=1.5:
        // ≈ 0.26·0.085 / (2.25·2.5) ≈ 0.00393
        let kappa_d = fx(74, 100);
        let gap = fx(85, 1000);
        let c = fx(225, 100);
        let kp = fx(15, 10);
        // just under the bound certifies; well over does not
        assert!(certified(fx(3, 1000), gap, kappa_d, c, kp));
        assert!(!certified(fx(10, 1000), gap, kappa_d, c, kp));
    }

    #[test]
    fn finalizes_requires_both_conditions() {
        let d = Domain::from_focus(
            vec![p(1), p(2), p(3), p(4)],
            vec![fx(85, 100), fx(5, 100), fx(5, 100), fx(5, 100)],
        );
        let (kappa_d, gap, c, kp) = (fx(74, 100), fx(85, 1000), fx(225, 100), fx(15, 10));

        // crosses threshold AND certified → Final
        assert_eq!(
            finalizes(fx(85, 100), &d, fx(3, 1000), gap, kappa_d, c, kp),
            Finality::Final
        );
        // crosses threshold but NOT certified (too much uncertified mass) → Pending
        assert_eq!(
            finalizes(fx(85, 100), &d, fx(50, 1000), gap, kappa_d, c, kp),
            Finality::Pending
        );
        // certified but below threshold → Pending
        assert_eq!(
            finalizes(fx(5, 100), &d, fx(3, 1000), gap, kappa_d, c, kp),
            Finality::Pending
        );
    }
}
