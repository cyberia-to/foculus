// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Fork-choice — the deterministic rule that completes the merge on conflict.
//!
//! A CRDT merge is undefined on a conflict group: it holds every member and
//! cannot pick one. `ForkChoice` is that missing function. Every honest node runs
//! the same strategy and MUST agree on the winner, so the rule is deterministic in
//! its inputs and nothing else — no wall clock, no local ordering, no randomness.
//!
//! The strategy is pluggable by trust model:
//!   - [`Serialize`] — single-writer: a conflict is impossible, so any conflict is
//!     a logic error surfaced loudly.
//!   - [`MinHash`]   — trusted multi-writer: deterministic tiebreak, zero φ*.
//!   - `Focus` (M2)  — trustless: stake-weighted φ*, feature-gated on `tru`.
//!
//! `MinHash` is protocol.md's own measure-zero exact-tie rule ("lower
//! hash(particle_data) wins") promoted to the general trusted-deployment rule: it
//! gives determinism without attack-resistance, which is exactly right when there
//! is no adversary to steer the outcome.

use crate::chain::{CyberlinkRecord, Signal};

/// Why a fork-choice could not name a winner.
#[derive(Debug, PartialEq, Eq)]
pub enum ForkError {
    /// A `Serialize` deployment saw a genuine conflict. Single-writer systems
    /// cannot produce one — this is a concurrency bug or the wrong strategy for a
    /// multi-writer deployment, surfaced rather than silently resolved.
    ConflictUnderSerialize,
    /// resolve() was handed an empty member set.
    Empty,
}

/// What a fork-choice strategy may read about the surrounding graph.
///
/// Trivial strategies ([`Serialize`], [`MinHash`]) ignore it — they need only the
/// conflict members. `Focus` uses it to build the tri-kernel graph and compute φ*,
/// which requires the whole cybergraph context, not just the two candidates. Kept
/// minimal on purpose: the only thing a strategy needs from the graph today is its
/// cyberlinks.
pub trait GraphView {
    /// Every cyberlink in the current graph context, in a deterministic order.
    fn links(&self) -> Vec<CyberlinkRecord>;
}

/// A concrete [`GraphView`] over an owned link set — built by the reconcile engine
/// from its index, and used directly in tests.
pub struct LinksView(pub Vec<CyberlinkRecord>);

impl GraphView for LinksView {
    fn links(&self) -> Vec<CyberlinkRecord> {
        self.0.clone()
    }
}

/// A deterministic rule picking one winner from a set of mutually-conflicting
/// signals. Implementations MUST be pure functions of the members and the graph
/// view — same inputs, same winner, on every node.
pub trait ForkChoice {
    /// Return the index into `members` of the winning signal. `members` are the
    /// distinct signals of one conflict group, in canonical `content_id` order
    /// (see [`crate::conflict::ConflictGroup::members`]). `view` is the surrounding
    /// graph — trivial strategies ignore it, `Focus` reads it.
    fn resolve(&self, members: &[Signal], view: &dyn GraphView) -> Result<usize, ForkError>;
}

/// Single-writer: conflicts are impossible by construction. Any conflict is a
/// logic error, surfaced as [`ForkError::ConflictUnderSerialize`] rather than
/// silently resolved — the honest failure a trusted single-writer wants.
pub struct Serialize;

impl ForkChoice for Serialize {
    fn resolve(&self, members: &[Signal], _view: &dyn GraphView) -> Result<usize, ForkError> {
        match members.len() {
            0 => Err(ForkError::Empty),
            1 => Ok(0),
            _ => Err(ForkError::ConflictUnderSerialize),
        }
    }
}

/// Trusted multi-writer: the member with the lowest `content_id` wins.
///
/// Deterministic on every node (content_id is canonical), independent of arrival
/// order, and needs no φ*. It picks *an* arbitrary-but-agreed winner, not a *fair*
/// one — correct precisely when there is no adversary trying to steer which member
/// wins. For the trustless case where steering must be resisted, use `Focus` (M2).
pub struct MinHash;

impl ForkChoice for MinHash {
    fn resolve(&self, members: &[Signal], _view: &dyn GraphView) -> Result<usize, ForkError> {
        if members.is_empty() {
            return Err(ForkError::Empty);
        }
        let mut best = 0usize;
        let mut best_id = members[0].content_id();
        for (i, m) in members.iter().enumerate().skip(1) {
            let id = m.content_id();
            if id < best_id {
                best = i;
                best_id = id;
            }
        }
        Ok(best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{CyberlinkRecord, Signal, SELF_NETWORK};

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
            box_moves: vec![],
            prev: [0u8; 32],
            step,
            height: 0,
            proof: None,
        }
    }

    fn empty_view() -> LinksView {
        LinksView(vec![])
    }

    #[test]
    fn serialize_accepts_singleton_rejects_conflict() {
        let f = Serialize;
        let v = empty_view();
        assert_eq!(f.resolve(&[sig(1, 0, 2, 3)], &v), Ok(0));
        assert_eq!(
            f.resolve(&[sig(1, 0, 2, 3), sig(1, 0, 4, 5)], &v),
            Err(ForkError::ConflictUnderSerialize)
        );
        assert_eq!(f.resolve(&[], &v), Err(ForkError::Empty));
    }

    #[test]
    fn minhash_is_deterministic_and_order_independent() {
        let f = MinHash;
        let v = empty_view();
        let a = sig(1, 0, 2, 3);
        let b = sig(1, 0, 4, 5);
        let ab = f.resolve(&[a.clone(), b.clone()], &v).unwrap();
        let ba = f.resolve(&[b.clone(), a.clone()], &v).unwrap();
        // the SAME signal wins regardless of the slice order
        let winner_ab = [a.clone(), b.clone()][ab].content_id();
        let winner_ba = [b.clone(), a.clone()][ba].content_id();
        assert_eq!(winner_ab, winner_ba);
        // and it is genuinely the minimum content_id
        let min = a.content_id().min(b.content_id());
        assert_eq!(winner_ab, min);
    }

    #[test]
    fn minhash_rejects_empty() {
        assert_eq!(MinHash.resolve(&[], &empty_view()), Err(ForkError::Empty));
    }

    /// Deterministic RNG (xorshift64), matching the seeding convention used by
    /// `tests/million.rs` — reproducible without external crates.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn shuffle<T>(&mut self, items: &mut [T]) {
            for i in (1..items.len()).rev() {
                let j = (self.next() as usize) % (i + 1);
                items.swap(i, j);
            }
        }
    }

    /// Property #8 ("no two nodes finalize conflicting state") depends on every
    /// node's fork-choice naming the identical single winner for a conflict group,
    /// however that node assembled its local view. `minhash_is_deterministic_and_
    /// order_independent` establishes this for two-way conflicts; here the same
    /// claim is checked for conflict groups from 2 to 16 members, each resolved
    /// under 50 random arrival orders, so agreement is not an artifact of the
    /// pairwise case.
    #[test]
    fn minhash_agrees_across_n_way_conflicts_and_many_arrival_orders() {
        let v = empty_view();
        let mut rng = Rng(0x9E3779B97F4A7C15);
        for n in 2..=16usize {
            let members: Vec<Signal> = (0..n)
                .map(|i| sig(1, 0, i as u8, (i as u8).wrapping_add(100)))
                .collect();
            let true_min = members
                .iter()
                .map(Signal::content_id)
                .min()
                .expect("nonempty group");

            let mut first_winner = None;
            for _ in 0..50 {
                let mut shuffled = members.clone();
                rng.shuffle(&mut shuffled);
                let idx = MinHash.resolve(&shuffled, &v).unwrap();
                let winner = shuffled[idx].content_id();
                assert_eq!(winner, true_min, "n={n}: winner must be the minimum content_id");
                match first_winner {
                    None => first_winner = Some(winner),
                    Some(w) => assert_eq!(
                        w, winner,
                        "n={n}: every arrival order must name the same winner"
                    ),
                }
            }
        }
    }
}
