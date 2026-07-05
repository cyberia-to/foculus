// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! The epoch randomness beacon b_E — `specs/beacon.md`.
//!
//! One unbiasable value, fixed after an epoch's finalized set is stable, that
//! seeds every settlement ordering and availability sample. It is one outer VDF
//! over entropy the protocol already commits:
//!
//!   b_E = VDF_T( H( sort{ finalized signal ids in S_E } ) )
//!
//! Why each property holds:
//!   - unpredictable — S_E is the foculus-finalized set, fixed only after the
//!     propose window closes; nothing of b_E is known while claims can be placed.
//!   - unbiasable — the aggregate depends on every finalized signal; one honest
//!     signal makes the inner hash unpredictable. sorting removes any last-mover
//!     advantage over ordering.
//!   - verifiable — recompute the inner hash and check the outer VDF.
//!   - live — a quiet or partitioned epoch (empty S_E) re-delays the previous
//!     value, so the beacon is always defined and always moves.
//!
//! This uses the finalized signals' content ids as the inner leaves (the merkle
//! clock restricted to S_E). Folding in the actual per-signal VDF proofs π_vdf_i
//! ([[vec]] P6) is the spec's exact form and a named refinement — same idea, a
//! commitment over the finalized set.
//!
//! The [`Beacon::value`] is a 32-byte particle — exactly what
//! [`crate::settlement`] consumes to seed its orderings.

use bbg::Particle;
use cyber_hemera::hash as hemera_hash;

use crate::vdf::{self, VdfProof};

/// An epoch beacon: the value plus the outer VDF proof that produced it.
#[derive(Clone, Debug)]
pub struct Beacon {
    /// The 32-byte beacon value b_E — seeds settlement orderings and DAS samples.
    pub value: Particle,
    /// The outer VDF proof, so any node can confirm the delay was spent.
    pub proof: VdfProof,
}

/// The inner commitment: H(sort{finalized ids}) over a non-empty epoch, or the
/// previous beacon value re-delayed when the epoch is quiet.
fn commitment(finalized: &[Particle], prev: &Particle) -> Particle {
    if finalized.is_empty() {
        return *prev;
    }
    let mut ids = finalized.to_vec();
    ids.sort(); // canonical — the aggregate is arrival-order independent
    let mut buf = Vec::with_capacity(ids.len() * 32);
    for id in &ids {
        buf.extend_from_slice(id);
    }
    hash32(&buf)
}

/// The beacon value from the commitment and the VDF output.
fn value_from(commitment: &Particle, vdf_output: u64) -> Particle {
    let mut buf = [0u8; 40];
    buf[..32].copy_from_slice(commitment);
    buf[32..].copy_from_slice(&vdf_output.to_le_bytes());
    hash32(&buf)
}

fn hash32(bytes: &[u8]) -> Particle {
    *hemera_hash(bytes)
        .as_bytes()
        .first_chunk::<32>()
        .unwrap_or(&[0u8; 32])
}

/// Compute the epoch beacon from its finalized set.
///
/// `finalized` is the set of finalized, non-conflicting signal ids (S_E stable to
/// depth d — the caller supplies the stable set). `t` is the outer VDF delay in
/// sequential squarings; it must exceed the settle decision window. `prev` is the
/// previous epoch's beacon value, used only when this epoch is quiet.
pub fn compute(finalized: &[Particle], t: u64, prev: &Particle) -> Beacon {
    let commitment = commitment(finalized, prev);
    let challenge = vdf::challenge_from_hash(&commitment);
    let proof = vdf::evaluate(challenge, t);
    let value = value_from(&commitment, proof.output);
    Beacon { value, proof }
}

/// Verify a beacon against the finalized set it claims to aggregate: re-derive the
/// commitment, check the outer VDF, and confirm the value. A pure function of the
/// finalized set — the same check every node runs.
pub fn verify(beacon: &Beacon, finalized: &[Particle], prev: &Particle) -> bool {
    let commitment = commitment(finalized, prev);
    let challenge = vdf::challenge_from_hash(&commitment);
    if beacon.proof.input != challenge {
        return false;
    }
    if !vdf::verify(&beacon.proof) {
        return false;
    }
    beacon.value == value_from(&commitment, beacon.proof.output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(b: u8) -> Particle {
        [b; 32]
    }

    const T: u64 = 64; // small delay for tests
    const PREV: Particle = [0xEE; 32];

    #[test]
    fn deterministic() {
        let s = [p(1), p(2), p(3)];
        let a = compute(&s, T, &PREV);
        let b = compute(&s, T, &PREV);
        assert_eq!(a.value, b.value, "same finalized set → same beacon");
    }

    #[test]
    fn order_independent() {
        // the finalized set is a set — permuting it must not change the beacon.
        let a = compute(&[p(1), p(2), p(3)], T, &PREV);
        let b = compute(&[p(3), p(1), p(2)], T, &PREV);
        assert_eq!(a.value, b.value, "sorting makes the aggregate order-free");
    }

    #[test]
    fn distinct_sets_give_distinct_beacons() {
        let a = compute(&[p(1), p(2)], T, &PREV);
        let b = compute(&[p(1), p(9)], T, &PREV);
        assert_ne!(a.value, b.value, "a different finalized set → a different beacon");
    }

    #[test]
    fn verify_accepts_and_rejects() {
        let s = [p(1), p(2), p(3)];
        let beacon = compute(&s, T, &PREV);
        assert!(verify(&beacon, &s, &PREV), "a valid beacon verifies");

        // tampered value
        let mut bad = beacon.clone();
        bad.value = p(0xAB);
        assert!(!verify(&bad, &s, &PREV), "a tampered value is rejected");

        // wrong finalized set
        assert!(!verify(&beacon, &[p(1), p(2)], &PREV), "wrong set is rejected");

        // tampered VDF output
        let mut bad_vdf = beacon.clone();
        bad_vdf.proof.output = bad_vdf.proof.output.wrapping_add(1);
        assert!(!verify(&bad_vdf, &s, &PREV), "a tampered VDF output is rejected");
    }

    #[test]
    fn quiet_epoch_re_delays_the_previous() {
        // empty finalized set → the beacon is VDF_T over the previous value, still
        // defined, still verifiable, and it moves.
        let prev = p(0x11);
        let beacon = compute(&[], T, &prev);
        assert!(verify(&beacon, &[], &prev), "a quiet-epoch beacon is valid");
        assert_ne!(beacon.value, prev, "the beacon advances even when idle");
    }

    #[test]
    fn a_single_honest_signal_changes_the_beacon() {
        // adding one finalized signal changes b_E — no actor controls the aggregate.
        let a = compute(&[p(1), p(2)], T, &PREV);
        let b = compute(&[p(1), p(2), p(3)], T, &PREV);
        assert_ne!(a.value, b.value);
    }
}
