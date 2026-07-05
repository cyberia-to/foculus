// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! The reconciliation engine — one seam, detection wired to a pluggable fork-choice.
//!
//! This is the merge completed: [`ConflictIndex`] detects, [`ForkChoice`] resolves.
//! Both resolution timings the design commits to are offered on the same engine and
//! MUST agree:
//!   - incremental — [`observe`](Reconciler::observe): resolve a group the moment a
//!     signal lands in it. Natural for `MinHash` / trusted deployments.
//!   - batch-at-epoch — [`resolve_all`](Reconciler::resolve_all): resolve every
//!     indexed conflict at once. Natural for `Focus`, which finalizes a whole
//!     domain's conflicts together.
//! Because the conflict group is canonically ordered and the fork-choice is
//! deterministic, both paths name the same winner for the same group.

use crate::chain::Signal;
use crate::conflict::{ConflictIndex, ConflictKey};
use crate::fork::{ForkChoice, ForkError};

use bbg::Particle;

/// One resolved conflict: which key, and the winning signal's content id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub key: ConflictKey,
    pub winner: Particle,
}

/// Detect-then-resolve, over a chosen [`ForkChoice`] strategy.
pub struct Reconciler<F: ForkChoice> {
    index: ConflictIndex,
    fork: F,
}

impl<F: ForkChoice> Reconciler<F> {
    pub fn new(fork: F) -> Self {
        Self {
            index: ConflictIndex::new(),
            fork,
        }
    }

    /// Incremental timing: index one signal; if it creates or joins a conflict,
    /// resolve that group now. Returns one [`Resolved`] per group this signal put
    /// into conflict (usually zero or one).
    pub fn observe(&mut self, sig: &Signal) -> Result<Vec<Resolved>, ForkError> {
        let keys = self.index.observe(sig);
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            out.push(self.resolve_key(&key)?);
        }
        Ok(out)
    }

    /// Batch timing: resolve every conflict currently indexed, in canonical key
    /// order. Idempotent and side-effect-free — safe to call at each epoch close.
    pub fn resolve_all(&self) -> Result<Vec<Resolved>, ForkError> {
        self.index
            .conflicts()
            .iter()
            .map(|key| self.resolve_key(key))
            .collect()
    }

    fn resolve_key(&self, key: &ConflictKey) -> Result<Resolved, ForkError> {
        let group = self.index.group(key).ok_or(ForkError::Empty)?;
        let members = group.members();
        let idx = self.fork.resolve(&members)?;
        Ok(Resolved {
            key: *key,
            winner: members[idx].content_id(),
        })
    }

    /// Read access to the underlying index (for finality / domain layers to build on).
    pub fn index(&self) -> &ConflictIndex {
        &self.index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{CyberlinkRecord, Signal, SELF_NETWORK};
    use crate::fork::MinHash;

    fn sig(neuron: u8, step: u64, from: u8, to: u8) -> Signal {
        Signal {
            neuron: [neuron; 32],
            network: SELF_NETWORK,
            links: vec![CyberlinkRecord {
                neuron: [neuron; 32],
                from: [from; 32],
                to: [to; 32],
                token: [0u8; 32],
                amount: 1,
                valence: 1,
                height: 0,
            }],
            delta_pi: vec![],
            prev: [0u8; 32],
            step,
            height: 0,
            proof: None,
        }
    }

    #[test]
    fn no_conflict_resolves_to_nothing() {
        let mut r = Reconciler::new(MinHash);
        assert!(r.observe(&sig(1, 0, 2, 3)).unwrap().is_empty());
        assert!(r.observe(&sig(1, 1, 2, 3)).unwrap().is_empty());
    }

    #[test]
    fn incremental_resolves_equivocation() {
        let mut r = Reconciler::new(MinHash);
        r.observe(&sig(1, 0, 2, 3)).unwrap();
        let resolved = r.observe(&sig(1, 0, 4, 5)).unwrap();
        assert_eq!(resolved.len(), 1);
        // winner is the lower content_id of the two
        let a = sig(1, 0, 2, 3).content_id();
        let b = sig(1, 0, 4, 5).content_id();
        assert_eq!(resolved[0].winner, a.min(b));
    }

    #[test]
    fn incremental_and_batch_agree() {
        // build two reconcilers over the same signals; resolve one incrementally,
        // one in batch. the winners must be identical — the core guarantee that
        // makes "both timings for a while" safe.
        let signals = [
            sig(1, 0, 2, 3),
            sig(1, 0, 4, 5), // equivocates with the first
            sig(2, 0, 6, 7),
            sig(2, 0, 8, 9), // equivocates with the third
            sig(3, 0, 1, 1), // lonely, no conflict
        ];

        let mut incremental = Reconciler::new(MinHash);
        let mut inc_winners: Vec<Resolved> = Vec::new();
        for s in &signals {
            inc_winners.extend(incremental.observe(s).unwrap());
        }

        let mut batch = Reconciler::new(MinHash);
        for s in &signals {
            // index the signals but ignore the per-signal (incremental) resolution;
            // all resolution happens once, below, at "epoch close"
            batch.observe(s).ok();
        }
        let batch_winners = batch.resolve_all().unwrap();

        // sort both by key for comparison (incremental order is arrival order)
        let mut inc_sorted = inc_winners.clone();
        inc_sorted.sort_by_key(|r| r.key);
        let mut batch_sorted = batch_winners.clone();
        batch_sorted.sort_by_key(|r| r.key);

        assert_eq!(inc_sorted, batch_sorted);
        assert_eq!(inc_sorted.len(), 2); // two equivocations, one lonely signal
    }

    #[test]
    fn serialize_engine_surfaces_conflict() {
        use crate::fork::Serialize;
        let mut r = Reconciler::new(Serialize);
        r.observe(&sig(1, 0, 2, 3)).unwrap();
        // the second signal at the same (neuron, step) is a conflict Serialize rejects
        assert_eq!(
            r.observe(&sig(1, 0, 4, 5)),
            Err(ForkError::ConflictUnderSerialize)
        );
    }
}
