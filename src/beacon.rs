// ---
// tags: foculus, rust, beacon, rewards, vdf
// crystal-type: source
// crystal-domain: cyber
// ---
//! Epoch beacon b_E — specs/beacon.md.
//!
//! Construction (live):
//! ```text
//! signal_root = H(sort{ π_vdf.output : i ∈ S_E })   // empty → zeros
//! if S_E empty:
//!   vdf_in = challenge(prev)
//! else:
//!   vdf_in = challenge(signal_root)
//! outer     = VDF_T(vdf_in)
//! b_E       = Hemera(domain ‖ epoch ‖ prev ‖ claims_root ‖ signal_root ‖ outer.output)
//! ```
//!
//! Claims must be frozen (`claims_root`) before the outer VDF runs so orderings
//! cannot be front-run. Quiet epochs re-delay `prev` (always live).

use cyber_hemera::hash as hemera_hash;

use crate::vdf::{self, VdfProof};

/// Domain separation for the beacon hash.
const DOMAIN: &[u8] = b"foculus-beacon-v0";

/// Genesis previous beacon (epoch 0 has no parent).
pub const GENESIS_PREV: [u8; 32] = [0u8; 32];

/// Default outer VDF delay (sequential squarings). Fits test time; product may raise.
pub const DEFAULT_OUTER_T: u64 = 256;

/// Fast outer delay for unit tests that only need the construction shape.
pub const TEST_OUTER_T: u64 = 32;

/// Full beacon artifact — verifiable by any node.
#[derive(Clone, Debug)]
pub struct BeaconArtifact {
    pub epoch: u64,
    pub prev: [u8; 32],
    pub claims_root: [u8; 32],
    /// H(sort of signal VDF outputs); zeros if quiet.
    pub signal_root: [u8; 32],
    /// Outer VDF proof (input → T squarings → output).
    pub outer_vdf: VdfProof,
    /// Final b_E consumed by settlement.
    pub beacon: [u8; 32],
}

/// Commit a set of claim ids into a claims root (propose freeze).
pub fn claims_root(claim_ids: &[[u8; 32]]) -> [u8; 32] {
    let mut sorted = claim_ids.to_vec();
    sorted.sort_unstable();
    let mut buf = Vec::with_capacity(8 + sorted.len() * 32);
    buf.extend_from_slice(&(sorted.len() as u64).to_le_bytes());
    for id in &sorted {
        buf.extend_from_slice(id);
    }
    hash32(&buf)
}

/// Aggregate sorted signal VDF outputs into `signal_root`.
pub fn signal_vdf_root(vdf_outputs: &[u64]) -> [u8; 32] {
    if vdf_outputs.is_empty() {
        return [0u8; 32];
    }
    let mut sorted = vdf_outputs.to_vec();
    sorted.sort_unstable();
    let mut buf = Vec::with_capacity(8 + sorted.len() * 8);
    buf.extend_from_slice(&(sorted.len() as u64).to_le_bytes());
    for o in &sorted {
        buf.extend_from_slice(&o.to_le_bytes());
    }
    hash32(&buf)
}

/// Collect outputs from verified signal VDF proofs (drops invalid).
pub fn collect_signal_outputs(proofs: &[VdfProof]) -> Vec<u64> {
    proofs
        .iter()
        .filter(|p| vdf::verify(p))
        .map(|p| p.output)
        .collect()
}

/// Run the outer VDF beacon after claims are frozen.
///
/// `signal_outputs` are π_vdf outputs of depth-stable finalized signals S_E.
/// Empty → quiet path: VDF_T(challenge(prev)).
pub fn open_beacon(
    epoch: u64,
    prev: &[u8; 32],
    claims_root: &[u8; 32],
    signal_outputs: &[u64],
    outer_t: u64,
) -> BeaconArtifact {
    let signal_root = signal_vdf_root(signal_outputs);
    let vdf_in = if signal_outputs.is_empty() {
        vdf::challenge_from_hash(prev)
    } else {
        vdf::challenge_from_hash(&signal_root)
    };
    let outer_vdf = vdf::evaluate(vdf_in, outer_t);
    let beacon = finalize_beacon(epoch, prev, claims_root, &signal_root, outer_vdf.output);
    BeaconArtifact {
        epoch,
        prev: *prev,
        claims_root: *claims_root,
        signal_root,
        outer_vdf,
        beacon,
    }
}

/// Verify a [`BeaconArtifact`]: re-check outer VDF and recompute b_E.
pub fn verify_beacon(art: &BeaconArtifact) -> bool {
    if !vdf::verify(&art.outer_vdf) {
        return false;
    }
    let expected_in = if art.signal_root == [0u8; 32] {
        vdf::challenge_from_hash(&art.prev)
    } else {
        vdf::challenge_from_hash(&art.signal_root)
    };
    if art.outer_vdf.input != expected_in {
        return false;
    }
    let b = finalize_beacon(
        art.epoch,
        &art.prev,
        &art.claims_root,
        &art.signal_root,
        art.outer_vdf.output,
    );
    b == art.beacon
}

/// Legacy pure-hash beacon (no VDF). Prefer [`open_beacon`] for production.
///
/// Kept for offline settle helpers that do not yet carry signal VDFs.
pub fn beacon(epoch: u64, prev: &[u8; 32], claims_root: &[u8; 32]) -> [u8; 32] {
    // Quiet outer VDF with default T so even the "simple" path is delayed.
    open_beacon(epoch, prev, claims_root, &[], DEFAULT_OUTER_T).beacon
}

/// Advance a beacon chain for sequential quiet epochs.
pub fn advance_empty(epoch: u64, prev: &[u8; 32]) -> [u8; 32] {
    open_beacon(epoch, prev, &[0u8; 32], &[], DEFAULT_OUTER_T).beacon
}

fn finalize_beacon(
    epoch: u64,
    prev: &[u8; 32],
    claims_root: &[u8; 32],
    signal_root: &[u8; 32],
    outer_output: u64,
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(DOMAIN.len() + 8 + 32 * 3 + 8);
    buf.extend_from_slice(DOMAIN);
    buf.extend_from_slice(&epoch.to_le_bytes());
    buf.extend_from_slice(prev);
    buf.extend_from_slice(claims_root);
    buf.extend_from_slice(signal_root);
    buf.extend_from_slice(&outer_output.to_le_bytes());
    hash32(&buf)
}

fn hash32(buf: &[u8]) -> [u8; 32] {
    *hemera_hash(buf)
        .as_bytes()
        .first_chunk::<32>()
        .unwrap_or(&[0u8; 32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_beacon_verifies() {
        let cr = claims_root(&[[1u8; 32], [2u8; 32]]);
        let art = open_beacon(1, &GENESIS_PREV, &cr, &[42, 7, 99], TEST_OUTER_T);
        assert!(verify_beacon(&art));
        assert_ne!(art.beacon, [0u8; 32]);
    }

    #[test]
    fn quiet_epoch_uses_prev() {
        let a = open_beacon(1, &GENESIS_PREV, &[0u8; 32], &[], TEST_OUTER_T);
        let b = open_beacon(1, &GENESIS_PREV, &[0u8; 32], &[], TEST_OUTER_T);
        assert_eq!(a.beacon, b.beacon);
        assert!(verify_beacon(&a));
        // next epoch chains
        let c = open_beacon(2, &a.beacon, &[0u8; 32], &[], TEST_OUTER_T);
        assert_ne!(c.beacon, a.beacon);
        assert!(verify_beacon(&c));
    }

    #[test]
    fn signal_set_changes_beacon() {
        let cr = claims_root(&[[9u8; 32]]);
        let a = open_beacon(1, &GENESIS_PREV, &cr, &[1, 2, 3], TEST_OUTER_T);
        let b = open_beacon(1, &GENESIS_PREV, &cr, &[1, 2, 4], TEST_OUTER_T);
        assert_ne!(a.beacon, b.beacon);
    }

    #[test]
    fn claims_order_independent() {
        let a = claims_root(&[[1u8; 32], [2u8; 32]]);
        let b = claims_root(&[[2u8; 32], [1u8; 32]]);
        assert_eq!(a, b);
    }

    #[test]
    fn tampered_vdf_fails() {
        let cr = claims_root(&[[1u8; 32]]);
        let mut art = open_beacon(1, &GENESIS_PREV, &cr, &[5], TEST_OUTER_T);
        art.outer_vdf.output ^= 1;
        assert!(!verify_beacon(&art));
    }

    #[test]
    fn invalid_signal_vdf_dropped() {
        let good = vdf::evaluate(7, 16);
        let mut bad = good.clone();
        bad.output ^= 1;
        let outs = collect_signal_outputs(&[good, bad]);
        assert_eq!(outs.len(), 1);
    }
}
