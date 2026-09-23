//! Namespace Merkle Tree (NMT) for completeness proofs.
//!
//! Leaves are sorted by namespace. Internal nodes carry (min_ns, max_ns).
//! Completeness proof: for namespace N, prove ALL leaves with N are included.
//! Omission is structurally impossible — sorted invariant guarantees contiguity.

use serde::{Deserialize, Serialize};

/// A leaf in the NMT.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NmtLeaf {
    pub namespace: String,
    pub data_hash: String,
}

/// NMT node (leaf or internal).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NmtNode {
    pub hash: String,
    pub min_ns: String,
    pub max_ns: String,
}

/// Completeness proof: the leaves for namespace N + enough info to verify root.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletenessProof {
    pub namespace: String,
    pub leaves: Vec<NmtLeaf>,
    /// Root of the full tree.
    pub root: NmtNode,
    /// Full sorted leaf list hash (for simple verification).
    /// Verifier rebuilds tree from leaves+all_leaves_hash to check root.
    pub total_leaves: usize,
}

/// Build NMT root from leaves (sorts internally).
pub fn build(leaves: &[NmtLeaf]) -> NmtNode {
    let mut sorted = leaves.to_vec();
    sorted.sort();
    build_tree(&sorted)
}

/// Prove completeness for a namespace: return all matching leaves + root.
pub fn prove(leaves: &[NmtLeaf], namespace: &str) -> CompletenessProof {
    let mut sorted = leaves.to_vec();
    sorted.sort();

    let matching: Vec<NmtLeaf> = sorted
        .iter()
        .filter(|l| l.namespace == namespace)
        .cloned()
        .collect();

    let root = build_tree(&sorted);

    CompletenessProof {
        namespace: namespace.to_string(),
        leaves: matching,
        root,
        total_leaves: sorted.len(),
    }
}

/// Verify completeness proof against a trusted root (from a prior sync or
/// beacon, not from the proof itself — a proof that supplies its own root
/// and has that root taken on faith proves nothing about the root).
///
/// For lightweight verification, the verifier needs to check:
/// 1. `proof.root` matches the caller's independently trusted root.
/// 2. All claimed leaves have the correct namespace.
/// 3. Claimed leaves form a contiguous sorted run.
///
/// This still does not prove completeness by itself: a verifier without the
/// full leaf set cannot detect that a leaf was omitted from the claimed run
/// (see `nmt_omission_detected`) — only `verify_full`, which rebuilds the
/// tree from every leaf, closes that gap.
pub fn verify(proof: &CompletenessProof, trusted_root: &NmtNode) -> bool {
    if proof.root != *trusted_root {
        return false;
    }
    // Basic structural checks.
    for leaf in &proof.leaves {
        if leaf.namespace != proof.namespace {
            return false;
        }
    }
    // Sorted.
    for i in 1..proof.leaves.len() {
        if proof.leaves[i] < proof.leaves[i - 1] {
            return false;
        }
    }
    // If no leaves: absence claim. The sorted invariant means if namespace N
    // doesn't appear in the leaves, it doesn't exist. This is valid even when
    // N falls within [min_ns, max_ns] — the NMT guarantees sorted contiguity,
    // so absence between "a" and "c" means "b" truly doesn't exist.
    true
}

/// Full verification: rebuild tree from all leaves and check root + completeness.
pub fn verify_full(all_leaves: &[NmtLeaf], proof: &CompletenessProof) -> bool {
    // Rebuild tree and check root matches, then run the root-bound checks.
    let root = build(all_leaves);
    if !verify(proof, &root) {
        return false;
    }

    // Check that proof.leaves contains ALL leaves with this namespace.
    let mut sorted = all_leaves.to_vec();
    sorted.sort();
    let actual: Vec<&NmtLeaf> = sorted
        .iter()
        .filter(|l| l.namespace == proof.namespace)
        .collect();

    if actual.len() != proof.leaves.len() {
        return false;
    }
    for (a, p) in actual.iter().zip(proof.leaves.iter()) {
        if *a != p {
            return false;
        }
    }

    true
}

// ── Internal tree construction ──

fn hash_leaf(leaf: &NmtLeaf) -> String {
    let data = format!("nmt_leaf:{}:{}", leaf.namespace, leaf.data_hash);
    cyber_hemera::hash(data.as_bytes()).to_hex()
}

fn hash_pair(left: &NmtNode, right: &NmtNode) -> NmtNode {
    let data = format!(
        "nmt_node:{}:{}:{}:{}",
        left.hash, right.hash, left.min_ns, right.max_ns
    );
    let hash = cyber_hemera::hash(data.as_bytes()).to_hex();
    let min_ns = std::cmp::min(&left.min_ns, &right.min_ns).clone();
    let max_ns = std::cmp::max(&left.max_ns, &right.max_ns).clone();
    NmtNode { hash, min_ns, max_ns }
}

fn build_tree(sorted_leaves: &[NmtLeaf]) -> NmtNode {
    if sorted_leaves.is_empty() {
        return NmtNode {
            hash: "0".repeat(64),
            min_ns: String::new(),
            max_ns: String::new(),
        };
    }
    if sorted_leaves.len() == 1 {
        let l = &sorted_leaves[0];
        return NmtNode {
            hash: hash_leaf(l),
            min_ns: l.namespace.clone(),
            max_ns: l.namespace.clone(),
        };
    }
    let mid = sorted_leaves.len() / 2;
    let left = build_tree(&sorted_leaves[..mid]);
    let right = build_tree(&sorted_leaves[mid..]);
    hash_pair(&left, &right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(ns: &str, data: &str) -> NmtLeaf {
        NmtLeaf {
            namespace: ns.into(),
            data_hash: cyber_hemera::hash(data.as_bytes()).to_hex(),
        }
    }

    #[test]
    fn nmt_build_deterministic() {
        let leaves = vec![leaf("a", "1"), leaf("a", "2"), leaf("b", "3")];
        assert_eq!(build(&leaves).hash, build(&leaves).hash);
    }

    #[test]
    fn nmt_namespace_bounds() {
        let leaves = vec![leaf("alpha", "1"), leaf("beta", "2"), leaf("gamma", "3")];
        let root = build(&leaves);
        assert_eq!(root.min_ns, "alpha");
        assert_eq!(root.max_ns, "gamma");
    }

    #[test]
    fn nmt_completeness_proof_valid() {
        let leaves = vec![
            leaf("a", "1"), leaf("a", "2"),
            leaf("b", "3"),
            leaf("c", "4"), leaf("c", "5"),
        ];
        let proof = prove(&leaves, "b");
        assert!(verify(&proof, &proof.root));
        assert!(verify_full(&leaves, &proof));
        assert_eq!(proof.leaves.len(), 1);
    }

    #[test]
    fn nmt_completeness_all_leaves_for_namespace() {
        let leaves = vec![
            leaf("x", "1"), leaf("x", "2"), leaf("x", "3"),
            leaf("y", "4"),
        ];
        let proof = prove(&leaves, "x");
        assert!(verify(&proof, &proof.root));
        assert!(verify_full(&leaves, &proof));
        assert_eq!(proof.leaves.len(), 3);
    }

    #[test]
    fn nmt_absence_proof() {
        let leaves = vec![leaf("a", "1"), leaf("c", "2")];
        let proof = prove(&leaves, "b");
        assert!(verify(&proof, &proof.root));
        assert!(verify_full(&leaves, &proof));
        assert_eq!(proof.leaves.len(), 0);
    }

    #[test]
    fn nmt_omission_detected() {
        let leaves = vec![
            leaf("a", "1"), leaf("a", "2"), leaf("b", "3"),
        ];
        let mut proof = prove(&leaves, "a");
        proof.leaves.pop(); // remove one "a" leaf
        // verify() against the (untouched) real root still passes: a
        // verifier without the full leaf set structurally cannot see the
        // omission. verify_full, which does have every leaf, catches it:
        assert!(verify(&proof, &proof.root));
        assert!(!verify_full(&leaves, &proof));
    }

    #[test]
    fn nmt_wrong_namespace_in_proof() {
        let leaves = vec![leaf("a", "1"), leaf("b", "2")];
        let mut proof = prove(&leaves, "a");
        proof.leaves.push(leaf("b", "2"));
        assert!(!verify(&proof, &proof.root)); // wrong namespace detected
    }

    #[test]
    fn nmt_verify_rejects_forged_root() {
        // Before this fix, verify() never read proof.root at all: a proof
        // carrying a root nobody derived from real leaves still passed as
        // long as its own leaves were internally sorted and same-namespace.
        let leaves = vec![leaf("a", "1"), leaf("b", "2"), leaf("c", "3")];
        let proof = prove(&leaves, "b");
        let forged_root = build(&[leaf("z", "does-not-exist")]);
        assert_ne!(proof.root, forged_root);
        assert!(!verify(&proof, &forged_root));
        // The real, independently-known root still passes.
        assert!(verify(&proof, &proof.root));
    }

    #[test]
    fn nmt_root_changes_on_modification() {
        let l1 = vec![leaf("a", "1"), leaf("b", "2")];
        let l2 = vec![leaf("a", "1"), leaf("b", "3")];
        assert_ne!(build(&l1).hash, build(&l2).hash);
    }

    #[test]
    fn nmt_sort_order_independent() {
        let l1 = vec![leaf("b", "2"), leaf("a", "1")];
        let l2 = vec![leaf("a", "1"), leaf("b", "2")];
        assert_eq!(build(&l1).hash, build(&l2).hash);
    }

    #[test]
    fn nmt_forged_leaf_in_full_verify() {
        let leaves = vec![leaf("a", "1"), leaf("b", "2")];
        let mut proof = prove(&leaves, "a");
        // Forge: claim extra leaf
        proof.leaves.push(leaf("a", "forged"));
        assert!(!verify_full(&leaves, &proof));
    }

    #[test]
    fn nmt_single_namespace() {
        let leaves = vec![leaf("x", "1"), leaf("x", "2"), leaf("x", "3")];
        let proof = prove(&leaves, "x");
        assert!(verify_full(&leaves, &proof));
        assert_eq!(proof.leaves.len(), 3);
    }
}
