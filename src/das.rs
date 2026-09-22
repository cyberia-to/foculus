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

    // Global root = hash of concatenated shard roots.
    let mut all_roots_bytes = Vec::with_capacity(shard_roots.len() * 32);
    for r in &shard_roots {
        all_roots_bytes.extend_from_slice(r.as_bytes());
    }
    let root = tree::root_hash(&all_roots_bytes);

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
pub fn verify_sample(sample: &Sample, commitment: &DasCommitment) -> bool {
    if sample.shard_index >= commitment.n {
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

/// A sampler's request that went unanswered — the peer did not return the
/// shard, whether because it withholds it or because it does not have it.
pub const WITHHELD: Option<Sample> = None;

/// The verdict of a sampling round: enough of the requested shards verified
/// to meet the confidence threshold, or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    Unavailable,
}

/// Decide availability from a round of sampling requests, where a withheld
/// or unanswered request is `None` rather than a `Sample`.
///
/// Each response counts as passed only if it is present and verifies
/// against the commitment; a withheld shard counts as a failed sample, the
/// same as a tampered one, so an adversary cannot avoid detection by
/// silence instead of forging data. `min_verified` is the number of passed
/// samples the round must reach — `confidence` is monotone in it, so a
/// confidence target translates to a count (20 for `confidence(20)`, about
/// 1 − 2⁻²⁰) and the verdict needs no floating point.
pub fn decide_availability(
    responses: &[Option<Sample>],
    commitment: &DasCommitment,
    min_verified: usize,
) -> Availability {
    let passed = responses
        .iter()
        .filter(|r| matches!(r, Some(s) if verify_sample(s, commitment)))
        .count();
    if passed >= min_verified {
        Availability::Available
    } else {
        Availability::Unavailable
    }
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

    #[test]
    fn all_present_shards_pass_as_available() {
        let data = b"every shard answers its sampling request";
        let k = 4;
        let n = 32;
        let shards = erasure::encode(data, k, n);
        let commitment = commit(&shards, k, data.len());

        let responses: Vec<Option<Sample>> = shards.iter().map(|s| Some(sample(s))).collect();
        assert_eq!(decide_availability(&responses, &commitment, 20), Availability::Available);
    }

    #[test]
    fn one_withheld_chunk_among_many_still_available() {
        let data = b"one silent peer should not sink an otherwise healthy sample round";
        let k = 4;
        let n = 32;
        let shards = erasure::encode(data, k, n);
        let commitment = commit(&shards, k, data.len());

        let responses: Vec<Option<Sample>> = shards
            .iter()
            .map(|s| if s.index == 7 { WITHHELD } else { Some(sample(s)) })
            .collect();
        assert_eq!(decide_availability(&responses, &commitment, 20), Availability::Available);
    }

    #[test]
    fn majority_withheld_flags_unavailable() {
        let data = b"an adversary hiding most of the data must not pass sampling";
        let k = 4;
        let n = 32;
        let shards = erasure::encode(data, k, n);
        let commitment = commit(&shards, k, data.len());

        // Withhold every shard past the first three: far below a 20-sample
        // threshold no matter how many are requested.
        let responses: Vec<Option<Sample>> = shards
            .iter()
            .map(|s| if s.index < 3 { Some(sample(s)) } else { WITHHELD })
            .collect();
        assert_eq!(decide_availability(&responses, &commitment, 20), Availability::Unavailable);
    }

    #[test]
    fn withheld_shard_counts_the_same_as_tampered() {
        let data = b"silence and forgery must be indistinguishable to the sampler";
        let k = 4;
        let n = 32;
        let shards = erasure::encode(data, k, n);
        let commitment = commit(&shards, k, data.len());

        let mut tampered = sample(&shards[0]);
        if !tampered.shard_data.is_empty() {
            tampered.shard_data[0] ^= 0xFF;
        }

        let honest_round: Vec<Option<Sample>> = shards.iter().map(|s| Some(sample(s))).collect();
        let withheld_round: Vec<Option<Sample>> = std::iter::once(WITHHELD)
            .chain(shards[1..].iter().map(|s| Some(sample(s))))
            .collect();
        let tampered_round: Vec<Option<Sample>> = std::iter::once(Some(tampered))
            .chain(shards[1..].iter().map(|s| Some(sample(s))))
            .collect();

        // Threshold at the edge — every requested shard must verify — so a
        // single failure of either kind is what flips the verdict.
        assert_eq!(decide_availability(&honest_round, &commitment, n), Availability::Available);
        assert_eq!(decide_availability(&withheld_round, &commitment, n), Availability::Unavailable);
        assert_eq!(decide_availability(&tampered_round, &commitment, n), Availability::Unavailable);
    }
}
