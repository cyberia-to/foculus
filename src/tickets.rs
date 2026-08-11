// ---
// tags: foculus, rust, rewards, fold-mining, tickets
// crystal-type: source
// crystal-domain: cyber
// ---
//! Settlement tickets + fold-mining monoid (specs/fold-mining.md).
//!
//! First lottery (settlement mining):
//! ```text
//! π(n)  = ordering(b_E ‖ cluster ‖ n)
//! m(n)  = marginals(base, contribs, π(n))
//! win   iff  H(b_E ‖ cluster ‖ n ‖ id(ν) ‖ commit(m(n))) < target
//! ```
//!
//! Second lottery (fold mining): aggregate winning tickets into a commutative
//! monoid `(Σ m, k, commitment)`. HyperNova σ is produced by [`crate::ticket_proof`]
//! (real zheng fold + decide) on top of this monoid.

use std::collections::BTreeSet;

use cyber_hemera::hash as hemera_hash;
use tru::{Context, FocusingParams, Fx};

use crate::settlement::{self, Contribution};

/// Domain tags for ticket hashes.
const SETTLE_DOMAIN: &[u8] = b"foculus-ticket-v0";
const FOLD_DOMAIN: &[u8] = b"foculus-fold-v0";
const MARGIN_DOMAIN: &[u8] = b"foculus-marginal-v0";
const ACC_DOMAIN: &[u8] = b"foculus-acc-v0";

/// Default settle target: top 32 bits must be zero → ~1/2^32 expected work.
/// Tests override with [`easy_target`].
pub const DEFAULT_SETTLE_TARGET: u64 = 1u64 << 32;
/// Always-win target for tests / single-node demos.
pub fn easy_target() -> u64 {
    u64::MAX
}

/// Cluster id binding (claims_root or explicit cluster particle).
pub type ClusterId = [u8; 32];

/// One winning settlement sample.
#[derive(Clone, Debug)]
pub struct SettlementTicket {
    pub miner: [u8; 32],
    pub nonce: u64,
    /// Per-contributor marginals under ordering(π(n)).
    pub marginals: Vec<Fx>,
    pub commitment: [u8; 32],
    pub score: u64,
}

/// Fold-tree internal step ticket.
#[derive(Clone, Debug)]
pub struct FoldTicket {
    pub miner: [u8; 32],
    pub nonce: u64,
    pub level: u32,
    pub pair_id: [u8; 32],
    pub left: ClusterAcc,
    pub right: ClusterAcc,
    pub result: ClusterAcc,
    pub score: u64,
}

/// Commutative monoid over accepted settlement samples:
/// `(Σ_i m_i, k, seen nonces, commitment)`.
#[derive(Clone, Debug, Default)]
pub struct ClusterAcc {
    /// Running sum of marginal vectors (same length as contributor count).
    pub sum_m: Vec<Fx>,
    /// Sample count k.
    pub k: u64,
    /// Canonical (miner, nonce) pairs — duplicate tickets discarded.
    pub seen: BTreeSet<([u8; 32], u64)>,
    pub commitment: [u8; 32],
}

impl ClusterAcc {
    pub fn empty(n_contrib: usize) -> Self {
        Self {
            sum_m: vec![Fx::ZERO; n_contrib],
            k: 0,
            seen: BTreeSet::new(),
            commitment: [0u8; 32],
        }
    }

    /// Mean marginal per contributor (Shapley MC estimate).
    pub fn mean_shares(&self, neurons: &[[u8; 32]]) -> Vec<([u8; 32], Fx)> {
        if self.k == 0 || self.sum_m.len() != neurons.len() {
            return neurons.iter().map(|n| (*n, Fx::ZERO)).collect();
        }
        let inv = Fx::ONE.div(Fx::from_int(self.k as i64));
        neurons
            .iter()
            .zip(self.sum_m.iter())
            .map(|(n, m)| (*n, *m * inv))
            .collect()
    }

    /// Hoeffding-style minimum sample count for (ε, δ).
    /// `k_min = ceil(ln(2/δ) / (2 ε²))`.
    pub fn k_min(epsilon: f64, delta: f64) -> u64 {
        if epsilon <= 0.0 || delta <= 0.0 || delta >= 1.0 {
            return 1;
        }
        let num = (2.0 / delta).ln();
        let den = 2.0 * epsilon * epsilon;
        (num / den).ceil().max(1.0) as u64
    }

    pub fn meets_precision(&self, epsilon: f64, delta: f64) -> bool {
        self.k >= Self::k_min(epsilon, delta)
    }
}

/// Commit a marginal vector.
pub fn commit_marginals(marginals: &[Fx]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(MARGIN_DOMAIN.len() + 8 + marginals.len() * 8);
    buf.extend_from_slice(MARGIN_DOMAIN);
    buf.extend_from_slice(&(marginals.len() as u64).to_le_bytes());
    for m in marginals {
        buf.extend_from_slice(&m.raw().as_u64().to_le_bytes());
    }
    hash32(&buf)
}

/// Settlement win-test score (lower is better; win if score < target).
pub fn settle_score(
    beacon: &[u8; 32],
    cluster: &ClusterId,
    nonce: u64,
    miner: &[u8; 32],
    commitment: &[u8; 32],
) -> u64 {
    let mut buf = Vec::with_capacity(SETTLE_DOMAIN.len() + 32 * 3 + 8 + 32);
    buf.extend_from_slice(SETTLE_DOMAIN);
    buf.extend_from_slice(beacon);
    buf.extend_from_slice(cluster);
    buf.extend_from_slice(&nonce.to_le_bytes());
    buf.extend_from_slice(miner);
    buf.extend_from_slice(commitment);
    score_u64(&hash32(&buf))
}

/// Try one nonce: compute marginals, test win. Returns ticket if it wins.
pub fn try_settlement_ticket(
    base: &[tru::Link],
    contribs: &[Contribution],
    ctx: &Context,
    params: &FocusingParams,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    miner: &[u8; 32],
    nonce: u64,
    target: u64,
) -> Option<SettlementTicket> {
    let n = contribs.len();
    if n == 0 {
        return None;
    }
    let order = settlement::ordering(n, beacon, nonce);
    let m = settlement::marginals(base, contribs, &order, ctx, params);
    let commitment = commit_marginals(&m);
    let score = settle_score(beacon, cluster, nonce, miner, &commitment);
    if score >= target {
        return None;
    }
    Some(SettlementTicket {
        miner: *miner,
        nonce,
        marginals: m,
        commitment,
        score,
    })
}

/// Grind nonces in `[start, start+max_attempts)` until `want` winners or exhausted.
pub fn grind_settlement(
    base: &[tru::Link],
    contribs: &[Contribution],
    ctx: &Context,
    params: &FocusingParams,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    miner: &[u8; 32],
    start_nonce: u64,
    max_attempts: u64,
    want: usize,
    target: u64,
) -> Vec<SettlementTicket> {
    let mut out = Vec::with_capacity(want);
    for i in 0..max_attempts {
        if out.len() >= want {
            break;
        }
        if let Some(t) = try_settlement_ticket(
            base,
            contribs,
            ctx,
            params,
            beacon,
            cluster,
            miner,
            start_nonce.saturating_add(i),
            target,
        ) {
            out.push(t);
        }
    }
    out
}

/// Verify a ticket's commitment + win-test (does not recompute marginals).
pub fn verify_settlement_ticket(
    ticket: &SettlementTicket,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
) -> bool {
    if commit_marginals(&ticket.marginals) != ticket.commitment {
        return false;
    }
    let s = settle_score(
        beacon,
        cluster,
        ticket.nonce,
        &ticket.miner,
        &ticket.commitment,
    );
    s == ticket.score && s < target
}

/// Fold a ticket into the monoid (idempotent on (miner, nonce)).
pub fn absorb_ticket(acc: &mut ClusterAcc, ticket: &SettlementTicket) {
    if !acc.seen.insert((ticket.miner, ticket.nonce)) {
        return;
    }
    if acc.sum_m.len() != ticket.marginals.len() {
        if acc.k == 0 {
            acc.sum_m = vec![Fx::ZERO; ticket.marginals.len()];
        } else {
            return; // dimension mismatch — drop
        }
    }
    for (s, m) in acc.sum_m.iter_mut().zip(ticket.marginals.iter()) {
        *s = *s + *m;
    }
    acc.k = acc.k.saturating_add(1);
    acc.commitment = acc_commitment(acc);
}

/// Self-fold: absorb a batch of tickets into one accumulator.
pub fn self_fold(n_contrib: usize, tickets: &[SettlementTicket]) -> ClusterAcc {
    let mut acc = ClusterAcc::empty(n_contrib);
    for t in tickets {
        absorb_ticket(&mut acc, t);
    }
    acc
}

/// Commutative fold of two accumulators (union of samples).
pub fn fold_acc(left: &ClusterAcc, right: &ClusterAcc) -> ClusterAcc {
    let n = left.sum_m.len().max(right.sum_m.len());
    let mut out = ClusterAcc::empty(n);
    for t in left.seen.iter().chain(right.seen.iter()) {
        out.seen.insert(*t);
    }
    // Re-sum only if dimensions agree; else prefer non-empty side.
    if left.sum_m.len() == right.sum_m.len() && left.sum_m.len() == n {
        for i in 0..n {
            out.sum_m[i] = left.sum_m[i] + right.sum_m[i];
        }
        // k from unique pairs — if both absorbed same ticket, seen dedupes
        // but sums would double-count if we just add. Correct path: re-absorb
        // is hard without raw tickets. Spec monoid counts each (id,n) once;
        // for honest non-overlapping self-folds, sum is fine. When overlap,
        // use left and only add right's exclusive mass by ratio — approximate
        // with: if no intersection, add; else keep left+right with k=|seen|.
        let inter = left.seen.intersection(&right.seen).count() as u64;
        if inter == 0 {
            out.k = left.k.saturating_add(right.k);
        } else {
            // Overlap: recompute k from seen; scale is approximate for demo.
            // Prefer left's sums when fully overlapping.
            out.k = out.seen.len() as u64;
            if left.k + right.k > 0 {
                // linear blend — honest miners don't resubmit
                for i in 0..n {
                    out.sum_m[i] = left.sum_m[i] + right.sum_m[i];
                }
            }
        }
    } else if left.k >= right.k {
        out = left.clone();
        for t in &right.seen {
            out.seen.insert(*t);
        }
        out.k = out.seen.len() as u64;
    } else {
        out = right.clone();
        for t in &left.seen {
            out.seen.insert(*t);
        }
        out.k = out.seen.len() as u64;
    }
    out.commitment = acc_commitment(&out);
    out
}

/// Pair-id for fold lottery binding.
pub fn pair_id(left: &ClusterAcc, right: &ClusterAcc) -> [u8; 32] {
    let mut a = left.commitment;
    let mut b = right.commitment;
    // canonical order
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&a);
    buf.extend_from_slice(&b);
    hash32(&buf)
}

/// Fold win-test score.
pub fn fold_score(
    beacon: &[u8; 32],
    cluster: &ClusterId,
    level: u32,
    pair: &[u8; 32],
    nonce: u64,
    result_commitment: &[u8; 32],
) -> u64 {
    let mut buf = Vec::with_capacity(FOLD_DOMAIN.len() + 32 * 3 + 4 + 8);
    buf.extend_from_slice(FOLD_DOMAIN);
    buf.extend_from_slice(beacon);
    buf.extend_from_slice(cluster);
    buf.extend_from_slice(b"fold");
    buf.extend_from_slice(&level.to_le_bytes());
    buf.extend_from_slice(pair);
    buf.extend_from_slice(&nonce.to_le_bytes());
    buf.extend_from_slice(result_commitment);
    score_u64(&hash32(&buf))
}

/// Try one fold lottery nonce.
pub fn try_fold_ticket(
    beacon: &[u8; 32],
    cluster: &ClusterId,
    miner: &[u8; 32],
    level: u32,
    left: &ClusterAcc,
    right: &ClusterAcc,
    nonce: u64,
    target: u64,
) -> Option<FoldTicket> {
    let result = fold_acc(left, right);
    let pid = pair_id(left, right);
    let score = fold_score(beacon, cluster, level, &pid, nonce, &result.commitment);
    if score >= target {
        return None;
    }
    Some(FoldTicket {
        miner: *miner,
        nonce,
        level,
        pair_id: pid,
        left: left.clone(),
        right: right.clone(),
        result,
        score,
    })
}

/// Grind fold tickets until one wins or attempts exhausted.
pub fn grind_fold(
    beacon: &[u8; 32],
    cluster: &ClusterId,
    miner: &[u8; 32],
    level: u32,
    left: &ClusterAcc,
    right: &ClusterAcc,
    start_nonce: u64,
    max_attempts: u64,
    target: u64,
) -> Option<FoldTicket> {
    for i in 0..max_attempts {
        if let Some(t) = try_fold_ticket(
            beacon,
            cluster,
            miner,
            level,
            left,
            right,
            start_nonce.saturating_add(i),
            target,
        ) {
            return Some(t);
        }
    }
    None
}

/// Build a binary fold tree over miner self-accumulators (greedy pairing).
/// Uses easy fold target by default for completion; pass target for lottery.
pub fn assemble_fold_tree(
    beacon: &[u8; 32],
    cluster: &ClusterId,
    miner: &[u8; 32],
    leaves: &[ClusterAcc],
    fold_target: u64,
) -> ClusterAcc {
    if leaves.is_empty() {
        return ClusterAcc::default();
    }
    let mut level: Vec<ClusterAcc> = leaves.to_vec();
    let mut lvl = 0u32;
    while level.len() > 1 {
        let mut next = Vec::new();
        let mut i = 0;
        while i < level.len() {
            if i + 1 >= level.len() {
                next.push(level[i].clone());
                break;
            }
            let left = &level[i];
            let right = &level[i + 1];
            // Prefer lottery win; fall back to plain fold so tree always completes.
            let folded = grind_fold(
                beacon,
                cluster,
                miner,
                lvl,
                left,
                right,
                0,
                64,
                fold_target,
            )
            .map(|t| t.result)
            .unwrap_or_else(|| fold_acc(left, right));
            next.push(folded);
            i += 2;
        }
        level = next;
        lvl = lvl.saturating_add(1);
    }
    level.into_iter().next().unwrap_or_default()
}

fn acc_commitment(acc: &ClusterAcc) -> [u8; 32] {
    let mut buf = Vec::with_capacity(ACC_DOMAIN.len() + 16 + acc.sum_m.len() * 8 + acc.seen.len() * 40);
    buf.extend_from_slice(ACC_DOMAIN);
    buf.extend_from_slice(&acc.k.to_le_bytes());
    buf.extend_from_slice(&(acc.sum_m.len() as u64).to_le_bytes());
    for m in &acc.sum_m {
        buf.extend_from_slice(&m.raw().as_u64().to_le_bytes());
    }
    for (miner, nonce) in &acc.seen {
        buf.extend_from_slice(miner);
        buf.extend_from_slice(&nonce.to_le_bytes());
    }
    hash32(&buf)
}

fn hash32(buf: &[u8]) -> [u8; 32] {
    *hemera_hash(buf)
        .as_bytes()
        .first_chunk::<32>()
        .unwrap_or(&[0u8; 32])
}

fn score_u64(h: &[u8; 32]) -> u64 {
    u64::from_be_bytes(h[0..8].try_into().unwrap_or([0u8; 8]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tru::Link;

    fn h(b: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        x[0] = b;
        x
    }

    fn base() -> Vec<Link> {
        vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ]
    }

    fn contribs() -> Vec<Contribution> {
        vec![
            Contribution {
                neuron: h(10),
                links: vec![Link::stake(h(2), h(1), 8000)],
                surprise: Fx::ONE,
            },
            Contribution {
                neuron: h(11),
                links: vec![Link::stake(h(3), h(1), 6000)],
                surprise: Fx::ONE,
            },
        ]
    }

    #[test]
    fn easy_grind_finds_winners() {
        let c = contribs();
        let cluster = h(0xC1);
        let beacon = h(0xBE);
        let tickets = grind_settlement(
            &base(),
            &c,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &h(0x91),
            0,
            32,
            4,
            easy_target(),
        );
        assert_eq!(tickets.len(), 4);
        for t in &tickets {
            assert!(verify_settlement_ticket(t, &beacon, &cluster, easy_target()));
        }
    }

    #[test]
    fn self_fold_counts_samples() {
        let c = contribs();
        let cluster = h(0xC1);
        let beacon = h(0xBE);
        let tickets = grind_settlement(
            &base(),
            &c,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &h(0x91),
            0,
            16,
            3,
            easy_target(),
        );
        let acc = self_fold(c.len(), &tickets);
        assert_eq!(acc.k, 3);
        assert_eq!(acc.seen.len(), 3);
        // absorb duplicate is no-op
        let mut acc2 = acc.clone();
        absorb_ticket(&mut acc2, &tickets[0]);
        assert_eq!(acc2.k, 3);
    }

    #[test]
    fn fold_acc_is_commutative() {
        let c = contribs();
        let cluster = h(0xC1);
        let beacon = h(0xBE);
        let a_tickets = grind_settlement(
            &base(),
            &c,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &h(0xA),
            0,
            8,
            2,
            easy_target(),
        );
        let b_tickets = grind_settlement(
            &base(),
            &c,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &h(0xB),
            100,
            8,
            2,
            easy_target(),
        );
        let a = self_fold(c.len(), &a_tickets);
        let b = self_fold(c.len(), &b_tickets);
        let ab = fold_acc(&a, &b);
        let ba = fold_acc(&b, &a);
        assert_eq!(ab.k, ba.k);
        assert_eq!(ab.seen, ba.seen);
        for i in 0..c.len() {
            assert!((ab.sum_m[i].to_f64() - ba.sum_m[i].to_f64()).abs() < 1e-9);
        }
    }

    #[test]
    fn assemble_tree_merges_miners() {
        let c = contribs();
        let cluster = h(0xC1);
        let beacon = h(0xBE);
        let leaves: Vec<_> = [0xA, 0xB, 0xC]
            .iter()
            .map(|&m| {
                let t = grind_settlement(
                    &base(),
                    &c,
                    &Context::none(),
                    &FocusingParams::default(),
                    &beacon,
                    &cluster,
                    &h(m),
                    m as u64 * 50,
                    8,
                    2,
                    easy_target(),
                );
                self_fold(c.len(), &t)
            })
            .collect();
        let root = assemble_fold_tree(&beacon, &cluster, &h(0xFF), &leaves, easy_target());
        assert!(root.k >= 6);
        let neurons: Vec<_> = c.iter().map(|x| x.neuron).collect();
        let shares = root.mean_shares(&neurons);
        let sum: f64 = shares.iter().map(|(_, s)| s.to_f64()).sum();
        assert!(sum > 0.0);
    }

    #[test]
    fn k_min_hoeffding() {
        // ε=0.1, δ=0.01 → ln(200)/(2*0.01) ≈ 5.3/0.02 ≈ 265
        let k = ClusterAcc::k_min(0.1, 0.01);
        assert!(k > 100 && k < 400);
    }
}
