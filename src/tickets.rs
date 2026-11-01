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
//! monoid `(Σ m, k, commitment)`. Each ticket's marginals are proven by
//! [`crate::ticket_proof`]; a cluster is checked ticket by ticket in
//! [`crate::cluster`] — the monoid shape stays for later accumulation.

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

/// Per-cluster settle target, banded by contributor count (specs/fold-mining.md
/// "settlement difficulty").
///
/// A settlement attempt's cost (`settlement::marginals`) is linear in the
/// cluster's contributor count `n_contrib`, so a flat target gives every
/// cluster equal win probability per attempt but not equal win probability
/// per unit of wall-clock time: a large cluster spends proportionally longer
/// per attempt and under-samples within the settlement window. Scaling the
/// target linearly with `n_contrib` against a baseline `base_n` keeps
/// expected time-to-win constant across cluster sizes. Saturates at
/// `u64::MAX` instead of overflowing for clusters far above the baseline.
pub fn banded_target(base_target: u64, n_contrib: usize, base_n: usize) -> u64 {
    let base_n = base_n.max(1) as u128;
    let n_contrib = n_contrib.max(1) as u128;
    let scaled = (base_target as u128).saturating_mul(n_contrib) / base_n;
    scaled.min(u64::MAX as u128) as u64
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

    /// Hoeffding-style minimum sample count for (ε, δ), in fixed point:
    /// `k_min = ceil(ln(2/δ) / (2 ε²))`. `Fx::ln` is Goldilocks integer
    /// arithmetic, so every node derives the same `k_min` — a fold-mining
    /// decide that this feeds must agree bit-for-bit, and `f64::ln` is not
    /// guaranteed to (libm differs across platforms).
    pub fn k_min(epsilon: Fx, delta: Fx) -> u64 {
        if epsilon <= Fx::ZERO || delta <= Fx::ZERO || delta >= Fx::ONE {
            return 1;
        }
        let ln_term = Fx::from_int(2).div(delta).ln();
        let den = Fx::from_int(2) * epsilon * epsilon;
        let ratio = ln_term.div(den);
        let floor = ratio.floor_to_i64().max(0);
        let k = if Fx::from_int(floor) == ratio { floor } else { floor + 1 };
        k.max(1) as u64
    }

    pub fn meets_precision(&self, epsilon: Fx, delta: Fx) -> bool {
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

/// Role separation (specs/rewards.md §7): a miner that is itself a contender
/// in the cluster it would settle can bias the sample by withholding a
/// nonce that lowers its own share, so the deterrent §7 names is that it
/// does not settle a cluster it contends in.
///
/// Not yet wired into [`try_settlement_ticket`]: gating there unconditionally
/// would starve settlement whenever the only miner available for a cluster
/// is also its sole contender (the common case for a single-node network,
/// today's every test in this crate and phase 1's early genesis nodes) — a
/// liveness regression the spec's forfeit-pricing deterrent avoids by
/// pricing the conflict instead of forbidding it outright. Wiring this in
/// needs that liveness trade-off decided first: exempt the single-miner
/// case, or accept unsettled clusters until a second miner appears.
pub fn role_separation_holds(miner: &[u8; 32], contribs: &[Contribution]) -> bool {
    !contribs.iter().any(|c| &c.neuron == miner)
}

/// Try one nonce: compute marginals, test win. Returns ticket if it wins.
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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
    if absorb_unsealed(acc, ticket) {
        acc.commitment = acc_commitment(acc);
    }
}

/// Fold many tickets, hashing the commitment once at the end — linear in
/// the number of tickets where repeated [`absorb_ticket`] is quadratic.
pub fn absorb_tickets<'a>(acc: &mut ClusterAcc, tickets: impl IntoIterator<Item = &'a SettlementTicket>) {
    let mut changed = false;
    for t in tickets {
        changed |= absorb_unsealed(acc, t);
    }
    if changed {
        acc.commitment = acc_commitment(acc);
    }
}

/// Add one ticket without resealing; `true` if it was absorbed.
fn absorb_unsealed(acc: &mut ClusterAcc, ticket: &SettlementTicket) -> bool {
    if acc.sum_m.len() != ticket.marginals.len() && acc.k != 0 {
        return false; // dimension mismatch — drop
    }
    if !acc.seen.insert((ticket.miner, ticket.nonce)) {
        return false;
    }
    if acc.sum_m.len() != ticket.marginals.len() {
        acc.sum_m = vec![Fx::ZERO; ticket.marginals.len()];
    }
    for (s, m) in acc.sum_m.iter_mut().zip(ticket.marginals.iter()) {
        *s = *s + *m;
    }
    acc.k = acc.k.saturating_add(1);
    true
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
        // Overlapping inputs double-count their shared tickets' sums: fold_acc
        // is exact only on disjoint sets. Settlement never trusts it — the
        // cluster sum is rebuilt from verified tickets (crate::cluster).
        let inter = left.seen.intersection(&right.seen).count() as u64;
        if inter == 0 {
            out.k = left.k.saturating_add(right.k);
        } else {
            // overlap: k from the union, sums approximate (see above)
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

/// Why [`fold_acc_checked`] refused to fold two accumulators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldOverlap {
    /// Number of `(miner, nonce)` pairs present in both accumulators.
    NonDisjoint(usize),
}

/// Fold two accumulators, rejecting overlap instead of mis-summing it.
///
/// specs/fold-mining.md requires the fold to be a commutative monoid that
/// "counts each canonical (id, n) pair once," with a re-submitted nonce
/// "detected at fold time and discarded." [`fold_acc`]'s overlap branch does
/// neither: `out.sum_m[i] = left.sum_m[i] + right.sum_m[i]` runs unconditionally,
/// so `fold_acc(x, x)` silently doubles every `sum_m` entry while `k` stays at
/// `x.k` — `mean_shares` then returns double the true mean, corrupting the
/// Shapley-share reward estimate `rewards.rs` mints against. A compact
/// `ClusterAcc` has no raw per-ticket marginals to subtract back out on
/// overlap, so there is no safe way to *compute* the right answer here; the
/// only sound move is to surface the overlap to the caller. In honest
/// operation two accumulators being folded together are built from disjoint
/// leaves of the cluster's fold tree, so `Ok` is the expected path and
/// `NonDisjoint` signals either a replayed ticket or a broken tree assembly.
pub fn fold_acc_checked(left: &ClusterAcc, right: &ClusterAcc) -> Result<ClusterAcc, FoldOverlap> {
    let inter = left.seen.intersection(&right.seen).count();
    if inter > 0 {
        return Err(FoldOverlap::NonDisjoint(inter));
    }
    let n = left.sum_m.len().max(right.sum_m.len());
    let mut out = ClusterAcc::empty(n);
    for t in left.seen.iter().chain(right.seen.iter()) {
        out.seen.insert(*t);
    }
    if left.sum_m.len() == right.sum_m.len() && left.sum_m.len() == n {
        for i in 0..n {
            out.sum_m[i] = left.sum_m[i] + right.sum_m[i];
        }
        out.k = left.k.saturating_add(right.k);
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
    Ok(out)
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
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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

/// specs/rewards.md §7 "Residual: withholding": a settlement miner that is
/// also a cluster contender can withhold a winning ticket that would lower
/// its own share; the injectable bias is bounded by its compute share `q`:
/// `bias(q) <= (mean - min) * q / (1 - q)`, where `range` is `mean - min`
/// over the cluster's published marginal shares (measured in
/// foculus/audit/withholding-bias.md).
///
/// `q` must be in `[0, 1)`; `q >= 1` is a majority attacker, which already
/// breaks consensus by other means and is out of scope for this bound.
pub fn withholding_bias_bound(range: Fx, q: Fx) -> Fx {
    debug_assert!(
        q < Fx::ONE,
        "compute share must be < 1; q >= 1 is a majority attack, out of scope for this bound"
    );
    range * q.div(Fx::ONE - q)
}

/// §7 prices the cheap deterrent: "a withheld ticket forfeits its subsidy;
/// calibrate so the forfeit exceeds the share-gain." `withholding_bias_bound`
/// gives the maximum share-gain a withholder can buy at compute share `q`;
/// scaled by the cluster's total settlement value it becomes a token amount.
/// Returns true iff `ticket_subsidy` forfeits more than that amount, i.e. the
/// deterrent is priced correctly for this cluster.
pub fn forfeit_deters_withholding(ticket_subsidy: Fx, range: Fx, q: Fx, cluster_value: Fx) -> bool {
    ticket_subsidy > withholding_bias_bound(range, q) * cluster_value
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
#[path = "tickets_tests.rs"]
mod tests;
