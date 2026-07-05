// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Conflict detection — a pure function of signal content, plus a monotonic index.
//!
//! Two signals conflict iff they share a [`ConflictKey`]. Detection is independent
//! of arrival order (protocol.md: "a pure function of particle content") and the
//! index is monotonic — once a conflict is observed it is never un-observed.
//!
//! Three conflict types are defined in protocol.md: double-spend (shared
//! nullifier), equivocation (same author + epoch + signal), and resource
//! collision (shared non-fungible input). Only equivocation is derivable from the
//! current [`Signal`], which carries no nullifier or resource-id — the other two
//! are extension points wired once that wire-format decision lands (plan M5). The
//! machinery here is generic over the key, so adding those keys later does not
//! touch this module's structure.

use std::collections::{BTreeMap, BTreeSet};

use cyber_hemera::hash as hemera_hash;

use bbg::Particle;

use crate::chain::{CyberlinkRecord, Signal};

/// A conflict key: two signals sharing one are mutually exclusive. 32 bytes so a
/// nullifier or a resource-id (both particles) drops in directly once available.
pub type ConflictKey = Particle;

/// The conflict keys a signal claims. Signals conflict iff their key sets intersect.
///
/// Today this is exactly the equivocation key. Double-spend and resource-collision
/// keys append here once the signal carries the nullifier / resource-id they need.
pub fn conflict_keys(sig: &Signal) -> Vec<ConflictKey> {
    vec![equivocation_key(sig)]
}

/// Equivocation key: hemera(neuron ‖ step). A neuron may hold at most one signal
/// per step; two distinct signals sharing (neuron, step) are an equivocation.
pub fn equivocation_key(sig: &Signal) -> ConflictKey {
    let mut buf = [0u8; 40]; // 32 + 8
    buf[..32].copy_from_slice(&sig.neuron);
    buf[32..40].copy_from_slice(&sig.step.to_le_bytes());
    let h = hemera_hash(&buf);
    *h.as_bytes().first_chunk::<32>().unwrap_or(&[0u8; 32])
}

/// A group of mutually-conflicting signals: everyone who claimed one key.
/// Members are keyed by `content_id`, so duplicates collapse and the set is
/// deterministically ordered (BTreeMap over the 32-byte id) on every node.
pub struct ConflictGroup {
    pub key: ConflictKey,
    members: BTreeMap<Particle, Signal>,
}

impl ConflictGroup {
    /// Members in canonical (content_id) order — identical on every node.
    pub fn members(&self) -> Vec<Signal> {
        self.members.values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

/// Monotonic conflict index: key → the distinct signals that have claimed it.
///
/// Deterministic by construction — `BTreeMap` everywhere, never `HashMap`, so
/// iteration and membership order are identical across nodes (determinism pass).
/// Accretes only: [`observe`](Self::observe) inserts, nothing removes.
#[derive(Default)]
pub struct ConflictIndex {
    groups: BTreeMap<ConflictKey, BTreeMap<Particle, Signal>>,
}

impl ConflictIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index a signal under each of its conflict keys. Returns the keys that, after
    /// this insertion, hold more than one distinct signal — i.e. the conflicts this
    /// signal just created or joined. A re-observed identical signal (same
    /// content_id) is idempotent and creates no conflict.
    pub fn observe(&mut self, sig: &Signal) -> Vec<ConflictKey> {
        let id = sig.content_id();
        let mut newly_conflicting = Vec::new();
        for key in conflict_keys(sig) {
            let group = self.groups.entry(key).or_default();
            group.insert(id, sig.clone());
            if group.len() > 1 {
                newly_conflicting.push(key);
            }
        }
        newly_conflicting
    }

    /// The conflict group for a key, if any signals have claimed it.
    pub fn group(&self, key: &ConflictKey) -> Option<ConflictGroup> {
        self.groups.get(key).map(|members| ConflictGroup {
            key: *key,
            members: members.clone(),
        })
    }

    /// Every key that currently holds a genuine conflict (>1 distinct signal), in
    /// canonical key order.
    pub fn conflicts(&self) -> Vec<ConflictKey> {
        self.groups
            .iter()
            .filter(|(_, m)| m.len() > 1)
            .map(|(k, _)| *k)
            .collect()
    }

    /// Every cyberlink across all indexed signals, each distinct signal counted
    /// once, in a deterministic order — the graph context a φ*-based fork-choice
    /// ([`crate::fork::GraphView`]) builds its tri-kernel graph from. Signals are
    /// deduplicated by `content_id` (a signal can sit in more than one conflict
    /// group once double-spend keys land) so no link is double-weighted.
    pub fn all_links(&self) -> Vec<CyberlinkRecord> {
        let mut seen: BTreeSet<Particle> = BTreeSet::new();
        let mut out = Vec::new();
        for group in self.groups.values() {
            for (id, sig) in group {
                if seen.insert(*id) {
                    out.extend(sig.links.iter().cloned());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::CyberlinkRecord;

    fn link(from: u8, to: u8) -> CyberlinkRecord {
        CyberlinkRecord {
            neuron: [1u8; 32],
            from: [from; 32],
            to: [to; 32],
            token: [0u8; 32],
            amount: 1,
            valence: 1,
            height: 0,
        }
    }

    fn sig(neuron: u8, step: u64, from: u8, to: u8) -> Signal {
        Signal {
            neuron: [neuron; 32],
            network: SELF_NETWORK,
            links: vec![link(from, to)],
            delta_pi: vec![],
            prev: [0u8; 32],
            step,
            height: 0,
            proof: None,
        }
    }

    use crate::chain::SELF_NETWORK;

    #[test]
    fn no_conflict_for_distinct_steps() {
        let mut ix = ConflictIndex::new();
        assert!(ix.observe(&sig(1, 0, 2, 3)).is_empty());
        assert!(ix.observe(&sig(1, 1, 2, 3)).is_empty());
        assert!(ix.conflicts().is_empty());
    }

    #[test]
    fn equivocation_detected_same_neuron_same_step() {
        let mut ix = ConflictIndex::new();
        assert!(ix.observe(&sig(1, 0, 2, 3)).is_empty());
        // same neuron, same step, DIFFERENT content — an equivocation
        let conflicted = ix.observe(&sig(1, 0, 4, 5));
        assert_eq!(conflicted.len(), 1);
        let g = ix.group(&conflicted[0]).unwrap();
        assert_eq!(g.len(), 2);
    }

    #[test]
    fn reobserving_identical_signal_is_idempotent() {
        let mut ix = ConflictIndex::new();
        let s = sig(1, 0, 2, 3);
        assert!(ix.observe(&s).is_empty());
        // exact same signal again — same content_id, no conflict
        assert!(ix.observe(&s).is_empty());
        assert!(ix.conflicts().is_empty());
    }

    #[test]
    fn detection_is_order_independent() {
        let a = sig(1, 0, 2, 3);
        let b = sig(1, 0, 4, 5);
        let mut ix1 = ConflictIndex::new();
        ix1.observe(&a);
        ix1.observe(&b);
        let mut ix2 = ConflictIndex::new();
        ix2.observe(&b);
        ix2.observe(&a);
        // same conflict key surfaced regardless of order
        assert_eq!(ix1.conflicts(), ix2.conflicts());
        // and the same members in the same canonical order
        let k = ix1.conflicts()[0];
        let m1: Vec<_> = ix1.group(&k).unwrap().members().iter().map(|s| s.content_id()).collect();
        let m2: Vec<_> = ix2.group(&k).unwrap().members().iter().map(|s| s.content_id()).collect();
        assert_eq!(m1, m2);
    }
}
