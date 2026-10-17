// ---
// tags: foculus, rust, rewards, fold-mining
// crystal-type: source
// crystal-domain: cyber
// ---
//! Cluster settlement: verify every ticket, then sum.
//!
//! Replaces the HyperNova fold seal, which the verifier never checked.
//! The cluster object keeps the commutative-monoid shape of
//! specs/fold-mining.md — `(Σ m, k, commitment of the accepted set)` in
//! [`ClusterAcc`] — so accumulation can replace the linear check later
//! without changing what a cluster means. Today verification is linear:
//! one ticket proof per accepted sample.

use std::collections::BTreeSet;

use crate::ticket_proof::{
    ProvenTicket, TicketError, TicketInputs, prove_ticket, verify_ticket,
};
use crate::tickets::{
    ClusterAcc, ClusterId, FoldTicket, absorb_tickets, fold_acc, fold_score, pair_id,
    try_settlement_ticket,
};

/// What a settle receipt carries: every accepted ticket with its proof and
/// the win-test target they were drawn under.
#[derive(Clone, Debug)]
pub struct ClusterProof {
    pub target: u64,
    pub tickets: Vec<ProvenTicket>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterError {
    Empty,
    /// Ticket at `index` failed verification.
    Ticket { index: usize, error: TicketError },
    /// The same `(miner, nonce)` appears twice.
    Duplicate { index: usize },
    /// The claimed accumulator differs from the one the tickets produce.
    Mismatch,
}

/// Strict cluster check (receipts): every ticket must verify, no duplicates.
/// Returns the accumulator the accepted set produces.
pub fn verify_cluster(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    proof: &ClusterProof,
) -> Result<ClusterAcc, ClusterError> {
    if proof.tickets.is_empty() {
        return Err(ClusterError::Empty);
    }
    let mut seen = BTreeSet::new();
    for (index, t) in proof.tickets.iter().enumerate() {
        if !seen.insert((t.ticket.miner, t.ticket.nonce)) {
            return Err(ClusterError::Duplicate { index });
        }
        verify_ticket(inputs, beacon, cluster, proof.target, t)
            .map_err(|error| ClusterError::Ticket { index, error })?;
    }
    Ok(cluster_acc(inputs.contribs.len(), proof))
}

/// Lenient aggregation (settlers): keep the tickets that verify, drop the
/// rest. Returns the accepted proof and the indices dropped.
pub fn accept_tickets(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
    tickets: &[ProvenTicket],
) -> (ClusterProof, Vec<usize>) {
    let mut seen = BTreeSet::new();
    let mut accepted = Vec::new();
    let mut dropped = Vec::new();
    for (index, t) in tickets.iter().enumerate() {
        let fresh = seen.insert((t.ticket.miner, t.ticket.nonce));
        if fresh && verify_ticket(inputs, beacon, cluster, target, t).is_ok() {
            accepted.push(t.clone());
        } else {
            dropped.push(index);
        }
    }
    (
        ClusterProof {
            target,
            tickets: accepted,
        },
        dropped,
    )
}

/// The accumulator a set of tickets produces.
pub fn cluster_acc(n_contrib: usize, proof: &ClusterProof) -> ClusterAcc {
    let mut acc = ClusterAcc::empty(n_contrib);
    absorb_tickets(&mut acc, proof.tickets.iter().map(|t| &t.ticket));
    acc
}

/// Rebuild and prove the tickets a peer's self-accumulator names.
///
/// A gossiped [`ClusterAcc`] carries sums nobody can check, but its
/// `(miner, nonce)` set determines every ticket: the settler redraws each
/// one from public data and proves it. A peer whose claimed sums differ
/// from the redrawn set is dropped whole (`Err(Mismatch)`), as is one that
/// names a losing nonce.
pub fn redraw_peer(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
    peer: &ClusterAcc,
) -> Result<Vec<ProvenTicket>, ClusterError> {
    let mut out = Vec::with_capacity(peer.seen.len());
    for (index, &(miner, nonce)) in peer.seen.iter().enumerate() {
        let ticket = try_settlement_ticket(
            inputs.base,
            inputs.contribs,
            inputs.ctx,
            inputs.params,
            beacon,
            cluster,
            &miner,
            nonce,
            target,
        )
        .ok_or(ClusterError::Ticket {
            index,
            error: TicketError::Lost,
        })?;
        let proven = prove_ticket(inputs, beacon, cluster, target, &ticket)
            .map_err(|error| ClusterError::Ticket { index, error })?;
        out.push(proven);
    }
    let redrawn = cluster_acc(
        inputs.contribs.len(),
        &ClusterProof {
            target,
            tickets: out.clone(),
        },
    );
    if redrawn.k != peer.k || redrawn.sum_m != peer.sum_m || redrawn.commitment != peer.commitment
    {
        return Err(ClusterError::Mismatch);
    }
    Ok(out)
}

/// Check a fold-lottery step (second lottery): the result is the monoid
/// fold of its inputs, the pair id and score are recomputed, the score wins.
///
/// This replaces σ_f. It checks the step, not its inputs: `left` and
/// `right` are trusted only once [`verify_cluster`] has produced them from
/// verified tickets.
pub fn verify_fold_ticket(
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
    ft: &FoldTicket,
) -> bool {
    let result = fold_acc(&ft.left, &ft.right);
    if result.commitment != ft.result.commitment
        || result.k != ft.result.k
        || result.sum_m != ft.result.sum_m
        || pair_id(&ft.left, &ft.right) != ft.pair_id
    {
        return false;
    }
    let score = fold_score(beacon, cluster, ft.level, &ft.pair_id, ft.nonce, &ft.result.commitment);
    score == ft.score && score < target
}

#[cfg(test)]
#[path = "cluster_tests.rs"]
mod tests;
