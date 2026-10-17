// ---
// tags: foculus, rust, rewards, fold-mining
// crystal-type: source
// crystal-domain: cyber
// ---
//! Ticket settlement and receipt verification.
//!
//! Settle: grind local winning tickets and prove each; redraw and prove the
//! tickets peers' self-accumulators name; keep what verifies; the cluster
//! accumulator is the sum over the accepted set; shares and the receipt
//! follow from it. Verify: the same pipeline from public inputs — every
//! ticket proof, the accumulator, the shares, the receipt hash. Nothing in
//! a receipt is taken on the settler's word.

use tru::{Context, FocusingParams, Link, impulse};

use crate::beacon::{claims_root, open_beacon, verify_beacon, TEST_OUTER_T};
use crate::cluster::{ClusterError, ClusterProof, accept_tickets, redraw_peer, verify_cluster};
use crate::rewards::{
    RewardClaim, RewardError, SettleReceipt, TicketPolicy, allocate_budget_pub,
    contributions_with_rho, receipt_hash_pub, verify_receipt,
};
use crate::settlement::{self, Contribution};
use crate::ticket_proof::{TicketInputs, prove_ticket};
use crate::tickets::{ClusterAcc, ClusterId, grind_settlement};

/// Public inputs needed to verify a settle receipt.
pub struct SettleVerifyInputs<'a> {
    pub base: &'a [Link],
    pub claims: &'a [RewardClaim],
    pub ctx: &'a Context,
    pub params: &'a FocusingParams,
    /// Protocol win-test target the tickets must beat.
    pub target: u64,
}

/// The public claim set a cluster settles over.
#[derive(Clone, Copy)]
pub struct ClaimSet<'a> {
    pub base: &'a [Link],
    pub claims: &'a [RewardClaim],
    pub ctx: &'a Context,
    pub params: &'a FocusingParams,
}

/// A settled cluster: the accepted proof and the accumulator it produces.
pub struct TicketSettlement {
    pub proof: ClusterProof,
    pub acc: ClusterAcc,
    /// Peer self-accumulators dropped because their tickets did not redraw.
    pub dropped_peers: usize,
}

/// Grind, prove, redraw peers, accept.
pub fn settle_cluster_tickets(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    policy: &TicketPolicy,
    peer_accs: &[ClusterAcc],
) -> Result<TicketSettlement, ClusterError> {
    let target = policy.settle_target;
    let mut tickets = Vec::new();
    for t in grind_settlement(
        inputs.base,
        inputs.contribs,
        inputs.ctx,
        inputs.params,
        beacon,
        cluster,
        &policy.miner,
        policy.start_nonce,
        policy.max_attempts,
        policy.want,
        target,
    ) {
        tickets.push(
            prove_ticket(inputs, beacon, cluster, target, &t)
                .map_err(|error| ClusterError::Ticket { index: tickets.len(), error })?,
        );
    }
    let mut dropped_peers = 0;
    for peer in peer_accs {
        match redraw_peer(inputs, beacon, cluster, target, peer) {
            Ok(redrawn) => tickets.extend(redrawn),
            Err(_) => dropped_peers += 1,
        }
    }
    let (proof, _duplicates) = accept_tickets(inputs, beacon, cluster, target, &tickets);
    let acc = verify_cluster(inputs, beacon, cluster, &proof)?;
    Ok(TicketSettlement {
        proof,
        acc,
        dropped_peers,
    })
}

/// Settle via settlement tickets (fold-mining path).
#[allow(clippy::too_many_arguments)]
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
    settle_with_peer_accs(epoch, prev_beacon, base, claims, ctx, params, budget, policy, &[])
}

/// Like [`settle_epoch_tickets`], also redrawing the tickets named by peer
/// self-accumulators (gossiped batches). Peers whose sums do not match
/// their redrawn tickets contribute nothing.
#[allow(clippy::too_many_arguments)]
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
    let set = ClaimSet { base, claims, ctx, params };
    let mut receipt = ticket_receipt(epoch, &art.beacon, &cr, set, budget, policy, peer_accs)?;
    receipt.beacon_artifact = Some(art);
    Ok(receipt)
}

/// Build a ticket-path receipt under an already opened beacon.
pub fn ticket_receipt(
    epoch: u64,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    set: ClaimSet<'_>,
    budget: u64,
    policy: &TicketPolicy,
    peer_accs: &[ClusterAcc],
) -> Result<SettleReceipt, RewardError> {
    let ClaimSet { base, claims, ctx, params } = set;
    let contribs = contributions_with_rho(claims);
    let inputs = TicketInputs { base, contribs: &contribs, ctx, params };
    let settled = settle_cluster_tickets(inputs, beacon, cluster, policy, peer_accs)
        .map_err(|_| RewardError::NoTickets)?;
    let directed_total = directed_total(base, claims, ctx, params);
    let shares = allocate_budget_pub(&settled.acc.mean_shares(&neurons(&contribs)), budget, directed_total)?;
    Ok(SettleReceipt {
        epoch,
        beacon: *beacon,
        claims_root: *cluster,
        receipt_hash: receipt_hash_pub(epoch, beacon, &shares),
        directed_total,
        budget,
        shares,
        beacon_artifact: None,
        settlement: Some(settled.proof),
        sample_count: settled.acc.k,
    })
}

fn neurons(contribs: &[Contribution]) -> Vec<[u8; 32]> {
    contribs.iter().map(|c| c.neuron).collect()
}

/// Δφ⁺ of the full coalition — the conservation ceiling.
pub fn directed_total(base: &[Link], claims: &[RewardClaim], ctx: &Context, params: &FocusingParams) -> tru::Fx {
    let all: Vec<Link> = claims.iter().flat_map(|c| c.links.clone()).collect();
    impulse(base, &all, ctx, params, params.epsilon).directed
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptError {
    /// Hash binding, beacon artifact or structure.
    Malformed,
    /// claims_root differs from the claim set.
    Claims,
    /// Δφ⁺ of the claim set differs from the receipt.
    DirectedTotal,
    /// Ticket proof target differs from the protocol target.
    Target,
    Cluster(ClusterError),
    /// Sample count differs from the accepted set.
    SampleCount,
    /// Recomputed shares give another receipt hash.
    Shares,
}

/// Verify a receipt from public inputs: claims, Δφ⁺, every ticket proof (or
/// the Monte-Carlo samples), the shares, the receipt hash.
pub fn verify_settlement(rec: &SettleReceipt, inp: &SettleVerifyInputs<'_>) -> Result<(), ReceiptError> {
    if !verify_receipt(rec) {
        return Err(ReceiptError::Malformed);
    }
    let ids: Vec<[u8; 32]> = inp.claims.iter().map(|c| c.id).collect();
    if inp.claims.is_empty() || claims_root(&ids) != rec.claims_root {
        return Err(ReceiptError::Claims);
    }
    let total = directed_total(inp.base, inp.claims, inp.ctx, inp.params);
    if total.raw() != rec.directed_total.raw() {
        return Err(ReceiptError::DirectedTotal);
    }
    let contribs = contributions_with_rho(inp.claims);
    let raw = match &rec.settlement {
        Some(proof) => {
            if proof.target != inp.target {
                return Err(ReceiptError::Target);
            }
            let inputs = TicketInputs { base: inp.base, contribs: &contribs, ctx: inp.ctx, params: inp.params };
            let acc = verify_cluster(inputs, &rec.beacon, &rec.claims_root, proof)
                .map_err(ReceiptError::Cluster)?;
            if acc.k != rec.sample_count {
                return Err(ReceiptError::SampleCount);
            }
            acc.mean_shares(&neurons(&contribs))
        }
        None => settlement::shapley(inp.base, &contribs, inp.ctx, inp.params, rec.sample_count, &rec.beacon),
    };
    let shares = allocate_budget_pub(&raw, rec.budget, total).map_err(|_| ReceiptError::Shares)?;
    if receipt_hash_pub(rec.epoch, &rec.beacon, &shares) != rec.receipt_hash {
        return Err(ReceiptError::Shares);
    }
    Ok(())
}

#[cfg(test)]
#[path = "settle_tickets_tests.rs"]
mod tests;
