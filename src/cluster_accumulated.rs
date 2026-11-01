// ---
// tags: foculus, rust, rewards, fold-mining, zheng, accumulation
// crystal-type: source
// crystal-domain: cyber
// ---
//! Cluster settlement as one zheng accumulation (feature
//! `accumulated-settlement`, off by default).
//!
//! Every ticket of a cluster is one statement of its [`TicketRelation`]
//! (`m[π_j] = v_j − v_{j−1}`). zheng `accumulate::ccs` runs each
//! statement's Spartan IOP down to one evaluation claim, accumulates the
//! claims `chunk` words per step and decides the last accumulator once: one
//! WHIR opening for the whole cluster instead of one certificate per ticket.
//!
//! What stays native, in this profile as in the public one:
//!
//! - statement checks (shape, commitment, score, win test, duplicates) —
//!   hashes over public data, recomputed;
//! - the coalition values `v_j`: no CCS for `tru::impulse` exists, so the
//!   values are disclosed, pinned in the relation and recomputed natively.
//!   Every column the relation reads is therefore public and the
//!   accumulated proof adds no soundness the native check does not already
//!   give. It is the plumbing a succinct cluster needs once impulse has a
//!   relation: then the values (and the impulse trace) become the committed
//!   witness and the native recomputation goes away.
//!
//! Without recursion the verifier checks every statement's IOP messages and
//! every accumulation step: verification is linear in the tickets
//! (zheng `audit/accumulation-2026-10.md` § why size is linear). Measured
//! against the native path in `audit/accumulated-settlement-2026-10.md`;
//! native [`crate::cluster::verify_cluster`] stays the default.

use std::collections::BTreeSet;

use lens::WhirParams;
use nebu::Goldilocks as F;
use zheng::accumulate::ccs::{self, Folded, Statement};

use crate::cluster::ClusterError;
use crate::proof::{ProofError, Relation};
use crate::ticket_proof::{TicketError, TicketInputs, TicketRelation, check_statement};
use crate::tickets::{ClusterAcc, ClusterId, SettlementTicket, absorb_tickets};

/// Protocol constants of the accumulated profile. Prover and verifier must
/// hold the same values; they are not carried in the proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccParams {
    /// WHIR parameters of every committed word and of the decider.
    pub whir: WhirParams,
    /// Statements accumulated per step (the accumulator rides along).
    pub chunk: usize,
}

impl AccParams {
    /// lens's default WHIR (rate 1/16, folding 4, Johnson, grinding 16),
    /// 64 statements per step.
    pub fn settlement() -> Self {
        Self {
            whir: WhirParams::default(),
            chunk: 64,
        }
    }
}

/// A cluster's tickets, their disclosed coalition values and one folded
/// proof of all their relations.
#[derive(Clone, Debug)]
pub struct AccumulatedCluster {
    pub target: u64,
    pub tickets: Vec<SettlementTicket>,
    /// `values[t][j] = v_j` of ticket `t` along its ordering.
    pub values: Vec<Vec<F>>,
    pub folded: Folded,
}

impl AccumulatedCluster {
    /// Proof bytes: the folded proof plus 8 bytes per disclosed value (the
    /// tickets themselves are not counted, as for the public profile).
    pub fn proof_bytes(&self) -> usize {
        self.folded.to_bytes().len() + 8 * self.values.iter().map(Vec::len).sum::<usize>()
    }
}

fn ticket_err(index: usize, error: TicketError) -> ClusterError {
    ClusterError::Ticket { index, error }
}

/// Statement checks and duplicates, shared by prover and verifier.
fn check_tickets(
    inputs: &TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
    tickets: &[SettlementTicket],
) -> Result<(), ClusterError> {
    if tickets.is_empty() {
        return Err(ClusterError::Empty);
    }
    let mut seen = BTreeSet::new();
    for (index, t) in tickets.iter().enumerate() {
        if !seen.insert((t.miner, t.nonce)) {
            return Err(ClusterError::Duplicate { index });
        }
        check_statement(inputs, beacon, cluster, target, t).map_err(|e| ticket_err(index, e))?;
    }
    Ok(())
}

struct Derived {
    instances: Vec<zheng::types::CCSInstance>,
    pins: Vec<Vec<(usize, F)>>,
    bytes: Vec<Vec<u8>>,
}

impl Derived {
    fn new(relations: &[TicketRelation<'_>], values: &[Vec<F>]) -> Self {
        Self {
            instances: relations.iter().map(Relation::instance).collect(),
            pins: relations
                .iter()
                .zip(values)
                .map(|(r, v)| r.disclosed_pins(v))
                .collect(),
            bytes: relations.iter().map(|r| r.statement().to_vec()).collect(),
        }
    }

    fn statements(&self) -> Vec<Statement<'_>> {
        self.instances
            .iter()
            .zip(&self.pins)
            .zip(&self.bytes)
            .map(|((instance, pins), bytes)| Statement {
                instance,
                pins: pins.clone(),
                bytes: bytes.clone(),
            })
            .collect()
    }
}

/// Prove a cluster's winning tickets as one accumulation.
pub fn prove_cluster_accumulated(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
    params: &AccParams,
    tickets: &[SettlementTicket],
) -> Result<AccumulatedCluster, ClusterError> {
    check_tickets(&inputs, beacon, cluster, target, tickets)?;
    let relations: Vec<_> = tickets
        .iter()
        .map(|t| TicketRelation::new(inputs, beacon, cluster, t))
        .collect();
    let values: Vec<Vec<F>> = relations.iter().map(TicketRelation::coalition_values).collect();
    let witnesses: Vec<Vec<F>> = relations
        .iter()
        .zip(&values)
        .map(|(r, v)| r.assignment(v))
        .collect();
    let derived = Derived::new(&relations, &values);
    let folded = ccs::fold(&params.whir, &derived.statements(), &witnesses, params.chunk)
        .map_err(|_| ClusterError::Accumulation)?;
    Ok(AccumulatedCluster {
        target,
        tickets: tickets.to_vec(),
        values,
        folded,
    })
}

/// Which part of [`verify_cluster_accumulated`] rejects; split for the
/// measurements (the native impulse recomputation dominates).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Statement checks, the folded proof.
    Proof,
    /// The native coalition-value recomputation.
    Native,
}

/// Verify the statement checks and the folded proof, not the coalition
/// values. Sound only together with [`check_values`].
pub fn verify_folded_part(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    params: &AccParams,
    proof: &AccumulatedCluster,
) -> Result<(), ClusterError> {
    check_tickets(&inputs, beacon, cluster, proof.target, &proof.tickets)?;
    let n = inputs.contribs.len();
    if proof.values.len() != proof.tickets.len() {
        return Err(ClusterError::Accumulation);
    }
    if let Some(index) = proof.values.iter().position(|v| v.len() != n) {
        return Err(ticket_err(index, TicketError::Shape));
    }
    let relations: Vec<_> = proof
        .tickets
        .iter()
        .map(|t| TicketRelation::new(inputs, beacon, cluster, t))
        .collect();
    let derived = Derived::new(&relations, &proof.values);
    ccs::verify_folded(&params.whir, &derived.statements(), &proof.folded, params.chunk)
        .map_err(|_| ClusterError::Accumulation)
}

/// Recompute every disclosed coalition value from `tru::impulse` (the
/// named gap: no CCS for impulse).
pub fn check_values(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    proof: &AccumulatedCluster,
) -> Result<(), ClusterError> {
    for (index, (t, values)) in proof.tickets.iter().zip(&proof.values).enumerate() {
        let relation = TicketRelation::new(inputs, beacon, cluster, t);
        if relation.coalition_values() != *values {
            return Err(ticket_err(index, TicketError::Proof(ProofError::WitnessCheck)));
        }
    }
    Ok(())
}

/// Strict cluster check in the accumulated profile: statement checks,
/// duplicates, the folded proof, then the native value recomputation.
/// Returns the accumulator the tickets produce — the same as
/// [`crate::cluster::verify_cluster`] on the same tickets.
pub fn verify_cluster_accumulated(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    params: &AccParams,
    proof: &AccumulatedCluster,
) -> Result<ClusterAcc, (Part, ClusterError)> {
    verify_folded_part(inputs, beacon, cluster, params, proof).map_err(|e| (Part::Proof, e))?;
    check_values(inputs, beacon, cluster, proof).map_err(|e| (Part::Native, e))?;
    let mut acc = ClusterAcc::empty(inputs.contribs.len());
    absorb_tickets(&mut acc, &proof.tickets);
    Ok(acc)
}

#[cfg(test)]
#[path = "cluster_accumulated_tests.rs"]
mod tests;
