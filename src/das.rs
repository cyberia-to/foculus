//! Data Availability Sampling (DAS).
//!
//! Commit erasure-coded shards via Hemera Merkle roots.
//! Verify availability by random sampling with inclusion proofs.

use cyber_hemera::Hash;
use cyber_hemera::tree;

use crate::erasure::Shard;

/// Commitment to a set of shards: one Merkle root per shard.
#[derive(Clone, Debug)]
pub struct DasCommitment {
    /// Hemera Merkle root of each shard's data.
    pub shard_roots: Vec<Hash>,
    /// Global root: hash of all shard roots.
    pub root: Hash,
    /// Parameters.
    pub k: usize,
    pub n: usize,
    /// Original data length in bytes.
    pub original_len: usize,
}

/// A sample: one shard's data + proof of inclusion.
#[derive(Clone, Debug)]
pub struct Sample {
    pub shard_index: usize,
    pub shard_data: Vec<u8>,
    pub shard_root: Hash,
}

/// Global root = hash of concatenated shard roots. Shared by `commit`
/// (which computes it) and `verify_sample` (which must recompute and check
/// it — `DasCommitment.root` is otherwise never read once the commitment is
/// off the wire, so a caller-supplied `shard_roots` vector could be spliced
/// with a forged entry at the sampled index while `root` is copied verbatim
/// from a genuine commitment, and no per-sample check alone would notice).
fn global_root(shard_roots: &[Hash]) -> Hash {
    let mut all_roots_bytes = Vec::with_capacity(shard_roots.len() * 32);
    for r in shard_roots {
        all_roots_bytes.extend_from_slice(r.as_bytes());
    }
    tree::root_hash(&all_roots_bytes)
}

/// Commit to a set of erasure-coded shards.
pub fn commit(shards: &[Shard], k: usize, original_len: usize) -> DasCommitment {
    let n = shards.len();
    let shard_roots: Vec<Hash> = shards
        .iter()
        .map(|s| {
            let bytes = shard_to_bytes(s);
            tree::root_hash(&bytes)
        })
        .collect();
    let root = global_root(&shard_roots);

    DasCommitment {
        shard_roots,
        root,
        k,
        n,
        original_len,
    }
}

/// Create a sample for a specific shard.
pub fn sample(shard: &Shard) -> Sample {
    let bytes = shard_to_bytes(shard);
    let root = tree::root_hash(&bytes);
    Sample {
        shard_index: shard.index,
        shard_data: bytes,
        shard_root: root,
    }
}

/// Verify a sample against a commitment.
///
/// Checks three things: the index is in range of the *actual* shard_roots
/// vector (not just the caller-supplied `n`), the commitment's `root` is the
/// hash of that exact `shard_roots` vector, and the sample data hashes to
/// the committed root at that index. The middle check is the binding one —
/// without it, `commitment.root` is a field that is set on construction and
/// never verified again, so any caller building a `DasCommitment` by hand
/// (or forwarding one from an untrusted peer) can attach an unrelated
/// `shard_roots` vector to a trusted `root` and every sample checks out
/// against the substituted entries.
pub fn verify_sample(sample: &Sample, commitment: &DasCommitment) -> bool {
    if sample.shard_index >= commitment.n || sample.shard_index >= commitment.shard_roots.len() {
        return false;
    }
    if global_root(&commitment.shard_roots) != commitment.root {
        return false;
    }

    // Check: hash of sample data matches committed shard root.
    let computed_root = tree::root_hash(&sample.shard_data);
    computed_root == commitment.shard_roots[sample.shard_index]
}

/// Verify availability by checking multiple random samples.
/// Returns (passed, total) — how many samples verified.
pub fn verify_availability(
    samples: &[Sample],
    commitment: &DasCommitment,
) -> (usize, usize) {
    let mut passed = 0;
    for s in samples {
        if verify_sample(s, commitment) {
            passed += 1;
        }
    }
    (passed, samples.len())
}

/// Confidence level for k successful samples out of k attempts.
/// 1 - (1/2)^k assuming adversary withholds >50%.
pub fn confidence(successful_samples: usize) -> f64 {
    1.0 - 0.5_f64.powi(successful_samples as i32)
}

/// Serialize a shard's field elements to bytes for hashing.
fn shard_to_bytes(shard: &Shard) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(shard.data.len() * 8);
    for &elem in &shard.data {
        bytes.extend_from_slice(&elem.as_u64().to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::erasure;

    #[test]
    fn commit_and_verify_samples() {
        let data = b"testing DAS commitment and sampling verification";
        let k = 2;
        let n = 4;
        let shards = erasure::encode(data, k, n);
        let commitment = commit(&shards, k, data.len());

        // Sample each shard and verify.
        for shard in &shards {
            let s = sample(shard);
            assert!(
                verify_sample(&s, &commitment),
                "shard {} failed verification",
                shard.index
            );
        }
    }

    #[test]
    fn tampered_sample_fails() {
        let data = b"tamper detection test";
        let k = 2;
        let n = 4;
        let shards = erasure::encode(data, k, n);
        let commitment = commit(&shards, k, data.len());

        let mut s = sample(&shards[0]);
        // Tamper with the data.
        if !s.shard_data.is_empty() {
            s.shard_data[0] ^= 0xFF;
        }
        assert!(!verify_sample(&s, &commitment));
    }

    #[test]
    fn spliced_shard_root_under_trusted_global_root_is_rejected() {
        // Two different files, each committed honestly.
        let data_a = b"the file a verifier actually has bytes for";
        let data_b = b"a completely different file an attacker wants credited";
        let k = 2;
        let n = 4;
        let shards_a = erasure::encode(data_a, k, n);
        let shards_b = erasure::encode(data_b, k, n);
        let commitment_a = commit(&shards_a, k, data_a.len());
        let commitment_b = commit(&shards_b, k, data_b.len());

        // Attacker keeps A's trusted global root, but forwards B's
        // shard_roots vector (e.g. relayed from an untrusted peer that
        // claims "here is the shard_roots for root R"). Before this fix,
        // verify_sample never checked shard_roots against root at all, so
        // a sample of B's data would verify under A's commitment.
        let mut forged = commitment_b.clone();
        forged.root = commitment_a.root;

        let s = sample(&shards_b[0]);
        assert!(
            !verify_sample(&s, &forged),
            "a shard_roots vector inconsistent with the trusted root must not verify"
        );

        // Sanity: the same sample verifies fine against its own honest commitment.
        assert!(verify_sample(&s, &commitment_b));
    }

    #[test]
    fn shard_index_beyond_actual_shard_roots_is_rejected() {
        // n says 4 shards, but shard_roots was truncated to 1 — an
        // out-of-bounds index must be rejected, not panic on indexing.
        let data = b"truncated shard_roots vector";
        let k = 2;
        let n = 4;
        let shards = erasure::encode(data, k, n);
        let mut commitment = commit(&shards, k, data.len());
        commitment.shard_roots.truncate(1);

        let s = sample(&shards[2]);
        assert!(!verify_sample(&s, &commitment));
    }

    #[test]
    fn confidence_calculation() {
        assert!(confidence(20) > 0.999999);
        assert!(confidence(30) > 0.999999999);
        assert!((confidence(1) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn availability_check() {
        let data = b"availability verification across all shards";
        let k = 2;
        let n = 4;
        let shards = erasure::encode(data, k, n);
        let commitment = commit(&shards, k, data.len());

        let samples: Vec<Sample> = shards.iter().map(|s| sample(s)).collect();
        let (passed, total) = verify_availability(&samples, &commitment);
        assert_eq!(passed, n);
        assert_eq!(total, n);
    }
}
