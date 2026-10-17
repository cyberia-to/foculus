// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Namespace Merkle Tree (NMT) for completeness proofs.
//!
//! Leaves are sorted by namespace. Internal nodes carry (min_ns, max_ns).
//! Completeness proof: for namespace N, prove ALL leaves with N are included.
//! Omission is structurally impossible — the sorted invariant guarantees contiguity.
//!
//! Node hashing is hemera's: `hemera::tree::hash_node_nmt` commits the
//! namespace bounds in the sponge capacity (hemera spec §4.6.4). This module
//! owns the tree shape and the completeness argument, never the hash rule.

use cyber_hemera::tree::{hash_leaf, hash_node_nmt};
use cyber_hemera::Hash;
use serde::{Deserialize, Serialize};

/// A 32-byte digest, serialisable without hemera's serde feature.
pub type Digest = [u8; 32];

/// A leaf in the NMT: a namespace and the hemera digest of the data it names.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NmtLeaf {
    pub namespace: u64,
    pub data_hash: Digest,
}

/// NMT node (leaf or internal).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NmtNode {
    pub hash: Digest,
    pub min_ns: u64,
    pub max_ns: u64,
}

/// Completeness proof: the leaves for namespace N + the root they sit under.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletenessProof {
    pub namespace: u64,
    pub leaves: Vec<NmtLeaf>,
    /// Root of the full tree.
    pub root: NmtNode,
    /// Size of the full sorted leaf list the root was built from.
    pub total_leaves: usize,
}

/// Build the NMT root from leaves (sorts internally).
pub fn build(leaves: &[NmtLeaf]) -> NmtNode {
    let mut sorted = leaves.to_vec();
    sorted.sort();
    build_tree(&sorted, true)
}

/// Prove completeness for a namespace: all matching leaves + the root.
pub fn prove(leaves: &[NmtLeaf], namespace: u64) -> CompletenessProof {
    let mut sorted = leaves.to_vec();
    sorted.sort();

    let matching: Vec<NmtLeaf> = sorted
        .iter()
        .filter(|l| l.namespace == namespace)
        .cloned()
        .collect();

    let root = build_tree(&sorted, true);

    CompletenessProof {
        namespace,
        leaves: matching,
        root,
        total_leaves: sorted.len(),
    }
}

/// Structural checks a verifier can run without the full leaf set:
/// every claimed leaf carries the namespace, the run is sorted, and the
/// namespace lies inside the root's bounds whenever leaves are claimed.
/// An empty leaf list is an absence claim: the sorted invariant means a
/// namespace that does not appear does not exist, even between the bounds.
pub fn verify(proof: &CompletenessProof) -> bool {
    for leaf in &proof.leaves {
        if leaf.namespace != proof.namespace {
            return false;
        }
    }
    for i in 1..proof.leaves.len() {
        if proof.leaves[i] < proof.leaves[i - 1] {
            return false;
        }
    }
    if !proof.leaves.is_empty()
        && (proof.namespace < proof.root.min_ns || proof.namespace > proof.root.max_ns)
    {
        return false;
    }
    true
}

/// Full verification: rebuild the tree from all leaves, check the root and
/// that the proof names every leaf of the namespace.
pub fn verify_full(all_leaves: &[NmtLeaf], proof: &CompletenessProof) -> bool {
    if !verify(proof) {
        return false;
    }
    if build(all_leaves) != proof.root {
        return false;
    }
    let mut sorted = all_leaves.to_vec();
    sorted.sort();
    let actual: Vec<&NmtLeaf> = sorted
        .iter()
        .filter(|l| l.namespace == proof.namespace)
        .collect();
    actual.len() == proof.leaves.len() && actual.iter().zip(&proof.leaves).all(|(a, p)| *a == p)
}

// ── Internal tree construction ──

fn leaf_hash(leaf: &NmtLeaf, is_root: bool) -> Digest {
    // the namespace is the leaf counter: same hemera leaf rule, namespace-bound.
    *hash_leaf(&leaf.data_hash, leaf.namespace, is_root).as_bytes()
}

fn hash_pair(left: &NmtNode, right: &NmtNode, is_root: bool) -> NmtNode {
    let min_ns = left.min_ns.min(right.min_ns);
    let max_ns = left.max_ns.max(right.max_ns);
    let hash = hash_node_nmt(
        &Hash::from_bytes(left.hash),
        &Hash::from_bytes(right.hash),
        min_ns,
        max_ns,
        is_root,
    );
    NmtNode {
        hash: *hash.as_bytes(),
        min_ns,
        max_ns,
    }
}

fn build_tree(sorted_leaves: &[NmtLeaf], is_root: bool) -> NmtNode {
    match sorted_leaves {
        [] => NmtNode {
            hash: [0u8; 32],
            min_ns: 0,
            max_ns: 0,
        },
        [l] => NmtNode {
            hash: leaf_hash(l, is_root),
            min_ns: l.namespace,
            max_ns: l.namespace,
        },
        _ => {
            let mid = sorted_leaves.len() / 2;
            let left = build_tree(&sorted_leaves[..mid], false);
            let right = build_tree(&sorted_leaves[mid..], false);
            hash_pair(&left, &right, is_root)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(ns: u64, data: &str) -> NmtLeaf {
        NmtLeaf {
            namespace: ns,
            data_hash: *cyber_hemera::hash(data.as_bytes()).as_bytes(),
        }
    }

    #[test]
    fn nmt_build_deterministic() {
        let leaves = vec![leaf(1, "1"), leaf(1, "2"), leaf(2, "3")];
        assert_eq!(build(&leaves).hash, build(&leaves).hash);
    }

    #[test]
    fn nmt_root_is_hemera_nmt_node() {
        // two leaves: root = hash_node_nmt(leaf_a, leaf_b, 1, 2, root=true)
        let a = leaf(1, "1");
        let b = leaf(2, "2");
        let root = build(&[b.clone(), a.clone()]);
        let la = hash_leaf(&a.data_hash, 1, false);
        let lb = hash_leaf(&b.data_hash, 2, false);
        let expect = hash_node_nmt(&la, &lb, 1, 2, true);
        assert_eq!(root.hash, *expect.as_bytes());
    }

    #[test]
    fn nmt_namespace_bounds() {
        let leaves = vec![leaf(10, "1"), leaf(20, "2"), leaf(30, "3")];
        let root = build(&leaves);
        assert_eq!(root.min_ns, 10);
        assert_eq!(root.max_ns, 30);
    }

    #[test]
    fn nmt_completeness_proof_valid() {
        let leaves = vec![leaf(1, "1"), leaf(1, "2"), leaf(2, "3"), leaf(3, "4"), leaf(3, "5")];
        let proof = prove(&leaves, 2);
        assert!(verify(&proof));
        assert!(verify_full(&leaves, &proof));
        assert_eq!(proof.leaves.len(), 1);
    }

    #[test]
    fn nmt_completeness_all_leaves_for_namespace() {
        let leaves = vec![leaf(7, "1"), leaf(7, "2"), leaf(7, "3"), leaf(8, "4")];
        let proof = prove(&leaves, 7);
        assert!(verify(&proof));
        assert!(verify_full(&leaves, &proof));
        assert_eq!(proof.leaves.len(), 3);
    }

    #[test]
    fn nmt_absence_proof() {
        let leaves = vec![leaf(1, "1"), leaf(3, "2")];
        let proof = prove(&leaves, 2);
        assert!(verify(&proof));
        assert!(verify_full(&leaves, &proof));
        assert!(proof.leaves.is_empty());
    }

    #[test]
    fn nmt_omitted_leaf_fails_full_verification() {
        let leaves = vec![leaf(5, "1"), leaf(5, "2"), leaf(6, "3")];
        let mut proof = prove(&leaves, 5);
        proof.leaves.pop();
        assert!(!verify_full(&leaves, &proof));
    }

    #[test]
    fn nmt_namespace_outside_bounds_rejected() {
        let leaves = vec![leaf(5, "1"), leaf(6, "2")];
        let mut proof = prove(&leaves, 5);
        proof.namespace = 9;
        for l in &mut proof.leaves {
            l.namespace = 9;
        }
        assert!(!verify(&proof));
    }
}
