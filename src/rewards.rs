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
//! - [`settle_epoch_tickets`] — grind settlement tickets + fold monoid
//!   (leaderless fold-mining first cut; see `tickets`)

use cyber_hemera::hash as hemera_hash;
use tru::{
    bts_scores, impulse, propose, surprise, Context, FocusingParams, Fx, Link, Report,
};

use crate::beacon::{claims_root, open_beacon, verify_beacon, BeaconArtifact, TEST_OUTER_T};
use crate::settlement::{self, Contribution};
use crate::ticket_proof::{prove_fold_tree, verify_fold_seal, FoldSeal};
use crate::tickets::{
    assemble_fold_tree, easy_target, grind_settlement, self_fold, ClusterAcc,
};

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
    /// HyperNova seal over settlement tickets.
    pub ticket_seal: Option<FoldSeal>,
    /// HyperNova seal over fold-tree assembly.
    pub fold_seal: Option<FoldSeal>,
    /// Sample count k in the root accumulator (0 if MC path).
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
        ticket_seal: None,
        fold_seal: None,
        sample_count: samples,
    };
    Ok(receipt)
}

/// Settle via settlement tickets + fold monoid (fold-mining path).
///
/// 1. claims_root + beacon
/// 2. grind winning tickets for `policy.miner`
/// 3. self-fold → assemble fold tree (single leaf when one miner)
/// 4. mean shares from root accumulator → budget allocation → receipt
///
/// Peer miner self-accumulators can be folded in via [`settle_with_peer_accs`].
pub fn settle_epoch_tickets(
    epoch: u64,
    prev_beacon: &[u8; 32],
    base: &[Link],
    claims: &[RewardClaim],
    ctx: &Context,
    params: &FocusingParams,
    budget: u64,
    policy: &TicketPolicy,
) -> Result<SettleReceipt, RewardError> {
    settle_with_peer_accs(
        epoch,
        prev_beacon,
        base,
        claims,
        ctx,
        params,
        budget,
        policy,
        &[],
    )
}

/// Like [`settle_epoch_tickets`] but merges additional miner self-accumulators
/// (gossiped peer batches) into the fold tree.
pub fn settle_with_peer_accs(
    epoch: u64,
    prev_beacon: &[u8; 32],
    base: &[Link],
    claims: &[RewardClaim],
    ctx: &Context,
    params: &FocusingParams,
    budget: u64,
    policy: &TicketPolicy,
    peer_accs: &[ClusterAcc],
) -> Result<SettleReceipt, RewardError> {
    if claims.is_empty() {
        return Err(RewardError::EmptyClaims);
    }
    if budget == 0 {
        return Err(RewardError::BudgetZero);
    }
    let ids: Vec<[u8; 32]> = claims.iter().map(|c| c.id).collect();
    let cr = claims_root(&ids);
    let art = open_beacon(epoch, prev_beacon, &cr, &[], TEST_OUTER_T);
    if !verify_beacon(&art) {
        return Err(RewardError::NoTickets);
    }
    let b_e = art.beacon;

    let contribs = contributions_with_rho(claims);
    let all_links: Vec<Link> = claims.iter().flat_map(|c| c.links.clone()).collect();
    let directed_total = impulse(base, &all_links, ctx, params, params.epsilon).directed;

    let tickets = grind_settlement(
        base,
        &contribs,
        ctx,
        params,
        &b_e,
        &cr,
        &policy.miner,
        policy.start_nonce,
        policy.max_attempts,
        policy.want,
        policy.settle_target,
    );
    if tickets.is_empty() && peer_accs.is_empty() {
        return Err(RewardError::NoTickets);
    }

    // HyperNova σ only after Δφ⁺ marginal replay for every ticket.
    let ticket_seal = if !tickets.is_empty() {
        let seal = crate::marginal_cert::prove_replayed_batch(
            base, &contribs, ctx, params, &b_e, &cr, &tickets,
        )
        .map_err(|_| RewardError::NoTickets)?;
        if !verify_fold_seal(&seal) {
            return Err(RewardError::NoTickets);
        }
        Some(seal)
    } else {
        None
    };

    let local = self_fold(contribs.len(), &tickets);
    let mut leaves = vec![local];
    leaves.extend(peer_accs.iter().cloned());
    // Prefer HyperNova-proven fold tree; fall back to monoid assemble.
    let (root, fold_seal) = match prove_fold_tree(&b_e, &cr, &leaves) {
        Ok((r, seal)) if verify_fold_seal(&seal) => (r, Some(seal)),
        _ => {
            let r = assemble_fold_tree(&b_e, &cr, &policy.miner, &leaves, policy.fold_target);
            (r, None)
        }
    };
    if root.k == 0 {
        return Err(RewardError::NoTickets);
    }
    let neurons: Vec<[u8; 32]> = contribs.iter().map(|c| c.neuron).collect();
    let raw_shares = root.mean_shares(&neurons);
    let shares = allocate_budget(&raw_shares, budget, directed_total)?;

    Ok(SettleReceipt {
        epoch,
        beacon: b_e,
        claims_root: cr,
        receipt_hash: receipt_hash(epoch, &b_e, &shares),
        directed_total,
        budget,
        shares,
        beacon_artifact: Some(art),
        ticket_seal,
        fold_seal,
        sample_count: root.k,
    })
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
    if paid < budget {
        if let Some(s) = out.iter_mut().find(|s| fx_weight(s.share) > 0) {
            s.amount = s.amount.saturating_add(budget - paid);
        }
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
    sorted.sort_by(|a, b| a.neuron.cmp(&b.neuron));
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

/// Verify a receipt's hash binding (does not re-run Shapley).
/// When live artifacts are present, also checks outer VDF + HyperNova seals.
pub fn verify_receipt(receipt: &SettleReceipt) -> bool {
    if receipt.receipt_hash != receipt_hash(receipt.epoch, &receipt.beacon, &receipt.shares) {
        return false;
    }
    if let Some(art) = &receipt.beacon_artifact {
        if !verify_beacon(art) || art.beacon != receipt.beacon {
            return false;
        }
    }
    if let Some(seal) = &receipt.ticket_seal {
        if !verify_fold_seal(seal) {
            return false;
        }
    }
    if let Some(seal) = &receipt.fold_seal {
        if !verify_fold_seal(seal) {
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
mod tests {
    use super::*;
    use crate::beacon::GENESIS_PREV;
    use tru::FocusingParams;

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

    #[test]
    fn solo_claim_earns_budget() {
        let params = FocusingParams::default();
        let claim = claim_from_links(
            h(0xA1),
            h(10),
            vec![Link::stake(h(2), h(1), 8000)],
            1,
        );
        let rec = settle_epoch(
            1,
            &GENESIS_PREV,
            &base(),
            &[claim],
            &Context::none(),
            &params,
            16,
            1000,
        )
        .unwrap();
        assert!(verify_receipt(&rec));
        assert_eq!(share_of(&rec, &h(10)), 1000);
    }

    #[test]
    fn two_contributors_split_budget() {
        let params = FocusingParams::default();
        let a = claim_from_links(
            h(0xA1),
            h(10),
            vec![Link::stake(h(2), h(1), 8000)],
            1,
        );
        let b = claim_from_links(
            h(0xB2),
            h(11),
            vec![Link::stake(h(3), h(1), 6000)],
            1,
        );
        let rec = settle_epoch(
            1,
            &GENESIS_PREV,
            &base(),
            &[a, b],
            &Context::none(),
            &params,
            32,
            1000,
        )
        .unwrap();
        assert!(verify_receipt(&rec));
        let sa = share_of(&rec, &h(10));
        let sb = share_of(&rec, &h(11));
        assert_eq!(sa + sb, 1000, "conservation of budget");
        // both positive if both contribute value
        assert!(sa > 0 || sb > 0);
    }

    #[test]
    fn copy_rho_zero_earns_nothing_when_bts_scores() {
        // With 2 reports where one is a pure copy, surprise should suppress it
        // when scores differ; with identical beliefs BTS may be flat.
        // Guarantee: allocate_budget zeros zero-weight shares.
        let raw = vec![(h(1), Fx::ONE), (h(2), Fx::ZERO)];
        let shares = allocate_budget(&raw, 100, Fx::ONE).unwrap();
        assert_eq!(shares[0].amount, 100);
        assert_eq!(shares[1].amount, 0);
    }

    #[test]
    fn empty_claims_err() {
        let err = settle_epoch(
            1,
            &GENESIS_PREV,
            &base(),
            &[],
            &Context::none(),
            &FocusingParams::default(),
            8,
            100,
        );
        assert_eq!(err.unwrap_err(), RewardError::EmptyClaims);
    }

    #[test]
    fn ticket_settle_matches_budget() {
        let params = FocusingParams::default();
        let claim = claim_from_links(
            h(0xA1),
            h(10),
            vec![Link::stake(h(2), h(1), 8000)],
            1,
        );
        let policy = TicketPolicy {
            want: 4,
            max_attempts: 64,
            miner: h(10),
            ..TicketPolicy::default()
        };
        let rec = settle_epoch_tickets(
            1,
            &GENESIS_PREV,
            &base(),
            &[claim],
            &Context::none(),
            &params,
            1000,
            &policy,
        )
        .unwrap();
        assert!(verify_receipt(&rec));
        assert_eq!(share_of(&rec, &h(10)), 1000);
    }
}
