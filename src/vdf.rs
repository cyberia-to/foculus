// ---
// tags: cybergraph, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Verifiable Delay Function over the Goldilocks field.
//!
//! Sequential squaring: output = x^(2^T) mod p.
//! Layer 2 ordering primitive — owned by cybergraph per structural-sync spec.
//! Proves minimum wall-clock time elapsed between signals; rate-limits equivocation.

use nebu::Goldilocks;

/// A VDF proof: input → T sequential squarings → output.
#[derive(Clone, Debug, PartialEq)]
pub struct VdfProof {
    /// Input to the VDF (derived from previous signal hash).
    pub input: u64,
    /// Output after T sequential squarings.
    pub output: u64,
    /// Number of sequential squarings (difficulty parameter).
    pub iterations: u64,
}

/// Evaluate the VDF: compute x^(2^T) by T sequential squarings.
///
/// Inherently sequential — cannot be parallelised. Time is proportional
/// to `iterations` regardless of hardware.
pub fn evaluate(input: u64, iterations: u64) -> VdfProof {
    let mut x = Goldilocks::new(input);
    if x.is_zero() {
        x = Goldilocks::ONE;
    }
    for _ in 0..iterations {
        x = x.square();
    }
    VdfProof { input, output: x.as_u64(), iterations }
}

/// Verify a VDF proof by re-evaluation.
///
/// For signal-scale (local sync), full re-evaluation is acceptable.
/// For global scale, Pietrzak/Wesolowski proofs would be needed.
///
/// That framing understates the gap: repeated squaring is a delay function
/// only in a group of *unknown* order (RSA, class groups), where computing
/// `x^(2^T)` with fewer than `T` sequential steps requires knowing the group
/// order to reduce the exponent. Goldilocks is a field with public, smooth
/// order `p − 1 = 2^32·(2^32 − 1)`, so `2^T mod (p − 1)` is cheap to compute
/// for any `T`, collapsing `evaluate`'s `T` sequential squarings to one
/// `Goldilocks::exp` call — see `fast_forward_matches_full_evaluation_for_any_t`
/// below and `audit/vdf-known-order-shortcut.md`. No `iterations` value
/// restores sequentiality here; the construction needs an unknown-order
/// group, not a larger `T`.
pub fn verify(proof: &VdfProof) -> bool {
    if proof.iterations == 0 {
        return proof.input == proof.output || (proof.input == 0 && proof.output == 1);
    }
    evaluate(proof.input, proof.iterations).output == proof.output
}

/// Derive a deterministic VDF challenge from a signal hash.
pub fn challenge_from_hash(hash: &[u8; 32]) -> u64 {
    let mut val: u64 = 0;
    for (i, &byte) in hash.iter().take(8).enumerate() {
        val |= (byte as u64) << (i * 8);
    }
    if val == 0 { 1 } else { val }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdf_evaluate_and_verify() {
        let proof = evaluate(42, 1000);
        assert!(verify(&proof));
        assert_ne!(proof.output, 42);
    }

    #[test]
    fn vdf_is_deterministic() {
        assert_eq!(evaluate(42, 1000), evaluate(42, 1000));
    }

    #[test]
    fn vdf_tampered_output_fails() {
        let mut proof = evaluate(42, 1000);
        proof.output ^= 1;
        assert!(!verify(&proof));
    }

    #[test]
    fn vdf_tampered_iterations_fails() {
        let mut proof = evaluate(42, 1000);
        proof.iterations += 1;
        assert!(!verify(&proof));
    }

    #[test]
    fn vdf_sequential_property() {
        let full = evaluate(7, 2000);
        let half = evaluate(evaluate(7, 1000).output, 1000);
        assert_eq!(full.output, half.output);
    }

    #[test]
    fn vdf_zero_iterations() {
        let proof = evaluate(42, 0);
        assert_eq!(proof.input, proof.output);
        assert!(verify(&proof));
    }

    /// `evaluate`'s `T` sequential squarings compute `input^(2^T) mod p`.
    /// Goldilocks's multiplicative group has public, smooth order
    /// `p − 1 = 2^32 · (2^32 − 1)`, so by Fermat/Euler
    /// `input^(2^T) ≡ input^(2^T mod (p − 1)) (mod p)` — a single
    /// `Goldilocks::exp` on the *reduced* exponent, no sequential loop
    /// needed. Checked here against real `evaluate` runs at `T` up to ~1e6
    /// (still near-instant to loop, unlike a real VDF at that `T`); the
    /// shortcut's own cost is one exponentiation regardless of `T`, so the
    /// gap between the two only widens as `T` grows past what this test runs.
    ///
    /// The property is even sharper than "there exists a shortcut": for
    /// `T ≥ 32`, `2^T mod (p − 1)` is periodic in `T` with period exactly
    /// 32 (`p − 1`'s odd factor `2^32 − 1` has `ord(2) | 32`, and `2^T ≡ 0
    /// (mod 2^32)` once `T ≥ 32`, so CRT fixes the residue from `T mod 32`
    /// alone). So `iterations` beyond 32 adds no additional unknowns to
    /// precompute — an attacker holds the full answer key in 32 entries.
    #[test]
    fn fast_forward_matches_full_evaluation_for_any_t() {
        let modulus = nebu::field::P - 1; // p − 1, the group order.
        for &(input, t) in &[(7u64, 10_000_000u64), (0x1234_5678, 1_000_032), (99, 63)] {
            let reduced = pow_mod_u64(2, t, modulus);
            let shortcut = Goldilocks::new(input).exp(reduced);
            let full = evaluate(input, t).output;
            assert_eq!(
                shortcut.as_u64(),
                full,
                "shortcut must match {t} real sequential squarings for input={input}"
            );
        }
    }

    #[test]
    fn fast_forward_exponent_has_period_32_past_t_equals_32() {
        let modulus = nebu::field::P - 1;
        for t in 32u64..96 {
            assert_eq!(
                pow_mod_u64(2, t, modulus),
                pow_mod_u64(2, t + 32, modulus),
                "2^T mod (p-1) must repeat every 32 once T >= 32 (T={t})"
            );
        }
    }

    /// `2^e mod m` by square-and-multiply in `u128` (test-only: independent
    /// of `Goldilocks::exp`, which operates mod `p`, not mod `p − 1`).
    fn pow_mod_u64(base: u64, mut e: u64, m: u64) -> u64 {
        let m = m as u128;
        let mut b = base as u128 % m;
        let mut acc = 1u128;
        while e > 0 {
            if e & 1 == 1 {
                acc = acc * b % m;
            }
            b = b * b % m;
            e >>= 1;
        }
        acc as u64
    }

    #[test]
    fn challenge_from_hash_is_deterministic() {
        let h = [0xab; 32];
        assert_eq!(challenge_from_hash(&h), challenge_from_hash(&h));
        assert_ne!(challenge_from_hash(&h), 0);
    }
}
