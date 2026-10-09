// ---
// tags: foculus, rust, rewards
// crystal-type: source
// crystal-domain: cyber
// ---
//! Closed reward pipeline: claim → ρ → Shapley settle → receipt.
//!
//! Connects:
//! - tru::impulse / propose (Δφ⁺)
//! - tru::truth_scoring (ρ)
//! - settlement::shapley (fair division under beacon)
//!
//! Produces a [`SettleReceipt`] that wallets mint against (clock B).
//!
//! Two settle paths:
//! - [`settle_epoch`] — in-process MC samples (trusted settler / CLI)
//! - [`settle_epoch_tickets`] — proven settlement tickets, linear cluster
//!   check (see `settle_tickets`, `cluster`)

use cyber_hemera::hash as hemera_hash;
use tru::{
    bts_scores, impulse, propose, surprise, Context, FocusingParams, Fx, Link, Report,
};

use crate::beacon::{claims_root, open_beacon, verify_beacon, BeaconArtifact, TEST_OUTER_T};
use crate::settlement::{self, Contribution};
use crate::cluster::ClusterProof;
use crate::tickets::easy_target;

pub use crate::settle_tickets::{settle_epoch_tickets, settle_with_peer_accs};

/// One neuron's reward claim for an epoch cluster (propose window).
#[derive(Clone)]
pub struct RewardClaim {
    /// Claim id (e.g. signal content_id).
    pub id: [u8; 32],
    pub neuron: [u8; 32],
    /// Links contributed this epoch.
    pub links: Vec<Link>,
    /// BTS first-order belief p ∈ [0,1] (link validity).
    pub belief: Fx,
    /// BTS meta-prediction m (valence mapped to [0,1]).
    pub prediction: Fx,
}

/// Settled share for one neuron.
#[derive(Clone, Debug)]
pub struct SettledShare {
    pub neuron: [u8; 32],
    /// Shapley share (fixed-point field).
    pub share: Fx,
    /// Token amount after budget allocation (conservation over budget).
    pub amount: u64,
}

/// Verifiable settle output — wallet mints from this, not free-form amounts.
#[derive(Clone, Debug)]
pub struct SettleReceipt {
    pub epoch: u64,
    pub beacon: [u8; 32],
    pub claims_root: [u8; 32],
    /// Hemera over (epoch ‖ beacon ‖ sorted (neuron, amount)).
    pub receipt_hash: [u8; 32],
    /// Total directed impulse of the full coalition (Δφ⁺ of all claims).
    pub directed_total: Fx,
    /// Budget units minted this epoch (caller policy; clipped to scale).
    pub budget: u64,
    pub shares: Vec<SettledShare>,
    /// Outer VDF beacon artifact (live path).
    pub beacon_artifact: Option<BeaconArtifact>,
    /// Every accepted settlement ticket with its proof (ticket path);
    /// `None` on the Monte-Carlo path, which verifiers recompute.
    pub settlement: Option<ClusterProof>,
    /// Sample count: accepted tickets, or Monte-Carlo samples.
    pub sample_count: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RewardError {
    EmptyClaims,
    NoPositiveShare,
    BudgetZero,
    /// Ticket grind produced no winning samples.
    NoTickets,
}

/// How many settlement tickets / grind budget for [`settle_epoch_tickets`].
#[derive(Clone, Debug)]
pub struct TicketPolicy {
    /// Winners wanted from the local miner.
    pub want: usize,
    /// Max nonces to try.
    pub max_attempts: u64,
    /// Starting nonce.
    pub start_nonce: u64,
    /// Win-test target (use [`tickets::easy_target`] for demos).
    pub settle_target: u64,
    pub fold_target: u64,
    /// Local miner id (usually the settler neuron).
    pub miner: [u8; 32],
}

impl Default for TicketPolicy {
    fn default() -> Self {
        Self {
            want: 8,
            max_attempts: 256,
            start_nonce: 0,
            settle_target: easy_target(),
            fold_target: easy_target(),
            miner: [0u8; 32],
        }
    }
}

/// Map valence ∈ {-1,0,+1} to a BTS meta-prediction in (0,1).
pub fn valence_to_prediction(v: i8) -> Fx {
    match v {
        1 => Fx::from_ratio(3, 4),
        -1 => Fx::from_ratio(1, 4),
        _ => Fx::from_ratio(1, 2),
    }
}

/// Build a claim from a neuron's links with default belief 3/4 (affirmative).
pub fn claim_from_links(
    id: [u8; 32],
    neuron: [u8; 32],
    links: Vec<Link>,
    valence: i8,
) -> RewardClaim {
    RewardClaim {
        id,
        neuron,
        links,
        belief: Fx::from_ratio(3, 4),
        prediction: valence_to_prediction(valence),
    }
}

/// Compute ρ for each claim via BTS leave-one-out, then Contribution list.
pub fn contributions_with_rho(claims: &[RewardClaim]) -> Vec<Contribution> {
    let reports: Vec<Report> = claims
        .iter()
        .map(|c| Report {
            neuron: c.neuron,
            belief: c.belief,
            prediction: c.prediction,
        })
        .collect();
    let scores = bts_scores(&reports);
    // s_max for surprise squash: max |score| among positive, else 1
    let mut s_max = Fx::ONE;
    for &s in &scores {
        if s > s_max {
            s_max = s;
        }
    }
    if s_max <= Fx::ZERO {
        s_max = Fx::ONE;
    }
    // Single-claim: BTS needs a crowd — ρ=1 so solo work mints.
    // Multi-claim with all-zero scores (no discrimination): ρ=1 for all (pay for Δφ⁺).
    // Otherwise squash BTS score to ρ.
    let n = claims.len();
    let mut rhos: Vec<Fx> = if n < 2 {
        vec![Fx::ONE]
    } else {
        scores.iter().map(|&s| surprise(s, s_max)).collect()
    };
    if n >= 2 && rhos.iter().all(|r| *r <= Fx::ZERO) {
        rhos = vec![Fx::ONE; n];
    }
    claims
        .iter()
        .enumerate()
        .map(|(i, c)| Contribution {
            neuron: c.neuron,
            links: c.links.clone(),
            surprise: rhos[i],
        })
        .collect()
}

/// Standalone propose claim (local first run) — ceiling among substitutes.
pub fn propose_claim(
    base: &[Link],
    claim: &RewardClaim,
    ctx: &Context,
    params: &FocusingParams,
) -> Fx {
    propose(base, &claim.links, ctx, params)
}

/// Run settlement for one cluster / epoch.
///
/// 1. claims_root from claim ids
/// 2. beacon(epoch, prev, claims_root)
/// 3. ρ-weighted contributions
/// 4. shapley MC under beacon
/// 5. allocate `budget` tokens proportional to positive shares
/// 6. clip: if directed_total is 0, no mint
#[allow(clippy::too_many_arguments)]
pub fn settle_epoch(
    epoch: u64,
    prev_beacon: &[u8; 32],
    base: &[Link],
    claims: &[RewardClaim],
    ctx: &Context,
    params: &FocusingParams,
    samples: u64,
    budget: u64,
) -> Result<SettleReceipt, RewardError> {
    if claims.is_empty() {
        return Err(RewardError::EmptyClaims);
    }
    if budget == 0 {
        return Err(RewardError::BudgetZero);
    }
    let ids: Vec<[u8; 32]> = claims.iter().map(|c| c.id).collect();
    let cr = claims_root(&ids);
    // Quiet outer VDF beacon (no signal set) — still delayed + verifiable.
    let art = open_beacon(epoch, prev_beacon, &cr, &[], TEST_OUTER_T);
    if !verify_beacon(&art) {
        return Err(RewardError::NoTickets); // reuse: beacon fail is fatal
    }
    let b_e = art.beacon;

    let contribs = contributions_with_rho(claims);
    // Full-coalition directed impulse (realized value ceiling)
    let all_links: Vec<Link> = claims.iter().flat_map(|c| c.links.clone()).collect();
    let directed_total = impulse(base, &all_links, ctx, params, params.epsilon).directed;

    let raw_shares = settlement::shapley(base, &contribs, ctx, params, samples, &b_e);
    let shares = allocate_budget(&raw_shares, budget, directed_total)?;

    let receipt = SettleReceipt {
        epoch,
        beacon: b_e,
        claims_root: cr,
        receipt_hash: receipt_hash(epoch, &b_e, &shares),
        directed_total,
        budget,
        shares,
        beacon_artifact: Some(art),
        settlement: None,
        sample_count: samples,
    };
    Ok(receipt)
}

/// Default tokens-per-Fx::ONE when mapping conserved mass without a tight budget.
/// High enough that `budget` remains the usual cap; conservation still clips field shares.
pub const DEFAULT_EMISSION_SCALE: u64 = 1_000_000_000;

/// Public wrapper for epoch runner — tok conservation + budget allocate.
pub fn allocate_budget_pub(
    raw: &[([u8; 32], Fx)],
    budget: u64,
    directed_total: Fx,
) -> Result<Vec<SettledShare>, RewardError> {
    allocate_budget(raw, budget, directed_total)
}

/// Public wrapper for epoch runner.
pub fn receipt_hash_pub(epoch: u64, beacon: &[u8; 32], shares: &[SettledShare]) -> [u8; 32] {
    receipt_hash(epoch, beacon, shares)
}

/// Tok conservation clip then proportional budget allocation.
///
/// 1. `tok::clip_shares` → renormalize to `min(v★, Δφ⁺)` (rewards §4)
/// 2. Split `budget` tokens by conserved weights (Σ amounts = budget when possible)
fn allocate_budget(
    raw: &[([u8; 32], Fx)],
    budget: u64,
    directed_total: Fx,
) -> Result<Vec<SettledShare>, RewardError> {
    let conserved = tok::clip_shares(raw, directed_total).map_err(|e| match e {
        tok::ConserveError::Empty => RewardError::EmptyClaims,
        tok::ConserveError::NoPositiveShare => RewardError::NoPositiveShare,
        tok::ConserveError::BudgetZero => RewardError::BudgetZero,
    })?;
    // Prefer tok conserve_and_allocate when emission_scale would bind under budget;
    // for settle receipts we always want budget as hard cap with conserved *weights*.
    let weights: Vec<u128> = conserved.iter().map(|(_, s)| fx_weight(*s)).collect();
    let sum_w: u128 = weights.iter().sum();
    if sum_w == 0 {
        return Err(RewardError::NoPositiveShare);
    }
    let mut out = Vec::with_capacity(conserved.len());
    let mut allocated = 0u64;
    for (i, (neuron, share)) in conserved.iter().enumerate() {
        let amt = if i + 1 == conserved.len() {
            budget.saturating_sub(allocated)
        } else {
            let a = ((weights[i] * budget as u128) / sum_w) as u64;
            allocated = allocated.saturating_add(a);
            a
        };
        out.push(SettledShare {
            neuron: *neuron,
            share: *share,
            amount: if weights[i] == 0 { 0 } else { amt },
        });
    }
    for (i, w) in weights.iter().enumerate() {
        if *w == 0 {
            out[i].amount = 0;
        }
    }
    let paid: u64 = out.iter().map(|s| s.amount).sum();
    if paid < budget
        && let Some(s) = out.iter_mut().find(|s| fx_weight(s.share) > 0)
    {
        s.amount = s.amount.saturating_add(budget - paid);
    }
    Ok(out)
}

/// Execute tok mint ledger from a settle receipt (PLUMB mint path).
pub fn mint_receipt_to_ledger(
    ledger: &mut tok::MintLedger,
    token: tok::TokenId,
    receipt: &SettleReceipt,
    emission_scale: u64,
) -> Result<tok::MintReceipt, tok::MintError> {
    let raw: Vec<_> = receipt
        .shares
        .iter()
        .map(|s| (s.neuron, s.share))
        .collect();
    // Re-run conservation with emission_scale; budget from receipt.
    tok::execute_settle_mints(
        ledger,
        token,
        &raw,
        receipt.directed_total,
        emission_scale,
        receipt.budget,
        &receipt.receipt_hash,
    )
}

/// Nonnegative weight for proportional split. Any strictly positive Fx maps
/// to at least 1 so tiny Shapley shares still receive budget mass.
fn fx_weight(x: Fx) -> u128 {
    if x <= Fx::ZERO {
        return 0;
    }
    let f = x.to_f64();
    if f <= 0.0 {
        return 0;
    }
    let w = (f * 1_000_000_000_000.0) as u128;
    w.max(1)
}

fn receipt_hash(epoch: u64, beacon: &[u8; 32], shares: &[SettledShare]) -> [u8; 32] {
    let mut sorted = shares.to_vec();
    sorted.sort_by_key(|s| s.neuron);
    let mut buf = Vec::with_capacity(8 + 32 + sorted.len() * 40);
    buf.extend_from_slice(&epoch.to_le_bytes());
    buf.extend_from_slice(beacon);
    for s in &sorted {
        buf.extend_from_slice(&s.neuron);
        buf.extend_from_slice(&s.amount.to_le_bytes());
    }
    *hemera_hash(&buf)
        .as_bytes()
        .first_chunk::<32>()
        .unwrap_or(&[0u8; 32])
}

/// Structural receipt check: hash binding, beacon artifact, and a ticket
/// set without duplicates whose size is the sample count.
///
/// Does not establish that shares are correct — that needs the claims and
/// base graph; use [`crate::settle_tickets::verify_settlement`].
pub fn verify_receipt(receipt: &SettleReceipt) -> bool {
    if receipt.receipt_hash != receipt_hash(receipt.epoch, &receipt.beacon, &receipt.shares) {
        return false;
    }
    if let Some(art) = &receipt.beacon_artifact
        && (!verify_beacon(art) || art.beacon != receipt.beacon)
    {
        return false;
    }
    if let Some(proof) = &receipt.settlement {
        let ids: std::collections::BTreeSet<_> =
            proof.tickets.iter().map(|t| (t.ticket.miner, t.ticket.nonce)).collect();
        if proof.tickets.is_empty()
            || ids.len() != proof.tickets.len()
            || ids.len() as u64 != receipt.sample_count
        {
            return false;
        }
    }
    true
}

/// Amount for a neuron in a receipt (0 if absent).
pub fn share_of(receipt: &SettleReceipt, neuron: &[u8; 32]) -> u64 {
    receipt
        .shares
        .iter()
        .find(|s| s.neuron == *neuron)
        .map(|s| s.amount)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "rewards_tests.rs"]
mod tests;
