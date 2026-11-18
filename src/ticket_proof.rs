// ---
// tags: foculus, rust, rewards, fold-mining, zheng
// crystal-type: source
// crystal-domain: cyber
// ---
//! Settlement ticket proofs (specs/fold-mining.md, tru/specs/rewards.md §7).
//!
//! The statement of a ticket `(ν, n, m, commit, score)` at beacon `b_E` for
//! cluster `C` over the public claim set:
//!
//! ```text
//! π(n)  = ordering(b_E, n)                                   statement check
//! m[π_j] = v_j − v_{j−1},  v_{−1} = 0,  j = 0..|contribs|    relation (CCS)
//! v_j   = v★(π_0..π_j) = impulse(base, prefix_j).directed    witness check
//! commit = commit(m)                                         statement check
//! score = H(b_E ‖ C ‖ n ‖ id(ν) ‖ commit) < target           statement check
//! ```
//!
//! The coalition values `v_j` are the witness: disclosed in the public
//! profile, checked against `tru::impulse` natively because no CCS for the
//! tri-kernel impulse exists yet. That native check is the gap a succinct
//! profile must close; until then verifying a ticket costs one impulse per
//! contributor — the same as recomputing it.

use nebu::Goldilocks as F;
use tru::{Context, FocusingParams, Link};
use zheng::types::{CCSInstance, CCSWitness};

use crate::proof::{self, Lin, Proof, ProofError, R1cs, Relation, Witnessed, digest};
use crate::settlement::{self, Contribution};
use crate::tickets::{ClusterId, SettlementTicket, commit_marginals, settle_score};

const STATEMENT_DOMAIN: &[u8] = b"foculus-ticket-proof-v1";

/// The public data every ticket of a cluster is checked against.
#[derive(Clone, Copy)]
pub struct TicketInputs<'a> {
    pub base: &'a [Link],
    pub contribs: &'a [Contribution],
    pub ctx: &'a Context,
    pub params: &'a FocusingParams,
}

/// A ticket and the proof of its marginals.
#[derive(Clone, Debug)]
pub struct ProvenTicket {
    pub ticket: SettlementTicket,
    pub proof: Proof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketError {
    /// Marginal vector length differs from the contributor count, or no contributors.
    Shape,
    /// `commit(m)` differs from the ticket's commitment.
    Commitment,
    /// Recomputed score differs from the ticket's score.
    Score,
    /// Score is not below the target.
    Lost,
    Proof(ProofError),
}

/// The relation of one ticket, derived from public data.
pub struct TicketRelation<'a> {
    inputs: TicketInputs<'a>,
    beacon: [u8; 32],
    cluster: ClusterId,
    ticket: &'a SettlementTicket,
    order: Vec<usize>,
}

impl<'a> TicketRelation<'a> {
    pub fn new(
        inputs: TicketInputs<'a>,
        beacon: &[u8; 32],
        cluster: &ClusterId,
        ticket: &'a SettlementTicket,
    ) -> Self {
        Self {
            order: settlement::ordering(inputs.contribs.len(), beacon, ticket.nonce),
            inputs,
            beacon: *beacon,
            cluster: *cluster,
            ticket,
        }
    }

    fn n(&self) -> usize {
        self.inputs.contribs.len()
    }

    /// Column of contributor `i`'s marginal.
    fn m_col(i: usize) -> usize {
        1 + i
    }

    /// Column of the coalition value after the first `j + 1` of π.
    fn v_col(&self, j: usize) -> usize {
        1 + self.n() + j
    }

    /// `v_j = v★(π_0..π_j)` for every prefix — the expensive part.
    pub(crate) fn coalition_values(&self) -> Vec<F> {
        let mut prefix: Vec<&Contribution> = Vec::with_capacity(self.n());
        self.order
            .iter()
            .map(|&i| {
                prefix.push(&self.inputs.contribs[i]);
                settlement::value(self.inputs.base, &prefix, self.inputs.ctx, self.inputs.params)
                    .raw()
            })
            .collect()
    }
}

impl Relation for TicketRelation<'_> {
    fn statement(&self) -> [u8; 32] {
        let n = (self.n() as u64).to_le_bytes();
        digest(
            STATEMENT_DOMAIN,
            &[
                &self.beacon,
                &self.cluster,
                &self.ticket.nonce.to_le_bytes(),
                &self.ticket.miner,
                &self.ticket.commitment,
                &n,
            ],
        )
    }

    fn instance(&self) -> CCSInstance {
        let n = self.n();
        let mut cs = R1cs::new(1 + 2 * n);
        for (j, &i) in self.order.iter().enumerate() {
            let mut row: Lin = vec![(self.v_col(j), F::ONE), (Self::m_col(i), -F::ONE)];
            if j > 0 {
                row.push((self.v_col(j - 1), -F::ONE));
            }
            cs.zero(row);
        }
        cs.instance()
    }

    fn public(&self) -> Vec<(usize, F)> {
        self.ticket
            .marginals
            .iter()
            .enumerate()
            .map(|(i, m)| (Self::m_col(i), m.raw()))
            .collect()
    }

    fn check_witness(&self, z: &[F]) -> bool {
        let values = self.coalition_values();
        values
            .iter()
            .enumerate()
            .all(|(j, v)| z.get(self.v_col(j)) == Some(v))
    }
}

impl TicketRelation<'_> {
    /// The full assignment for coalition values `values` (ordering position
    /// `j` ↦ `v_j`).
    pub(crate) fn assignment(&self, values: &[F]) -> Vec<F> {
        let mut z = vec![F::ZERO; self.instance().num_cols];
        z[0] = F::ONE;
        for (col, v) in self.public() {
            z[col] = v;
        }
        for (j, &v) in values.iter().enumerate() {
            z[self.v_col(j)] = v;
        }
        z
    }

    /// Pins of the accumulated profile: `(0, 1)`, the marginals and the
    /// disclosed coalition values, strictly increasing. Every column the
    /// relation reads is pinned: the values are public because the native
    /// impulse check reads them (no CCS for impulse exists yet).
    pub(crate) fn disclosed_pins(&self, values: &[F]) -> Vec<(usize, F)> {
        let mut pins = vec![(0, F::ONE)];
        pins.extend(self.public());
        pins.extend(values.iter().enumerate().map(|(j, &v)| (self.v_col(j), v)));
        pins
    }
}

impl Witnessed for TicketRelation<'_> {
    fn witness(&self) -> Option<CCSWitness> {
        Some(CCSWitness {
            z: self.assignment(&self.coalition_values()),
        })
    }
}

/// Statement checks: shape, commitment, score and win test.
pub(crate) fn check_statement(
    inputs: &TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
    ticket: &SettlementTicket,
) -> Result<(), TicketError> {
    let n = inputs.contribs.len();
    if n == 0 || ticket.marginals.len() != n {
        return Err(TicketError::Shape);
    }
    if commit_marginals(&ticket.marginals) != ticket.commitment {
        return Err(TicketError::Commitment);
    }
    let score = settle_score(beacon, cluster, ticket.nonce, &ticket.miner, &ticket.commitment);
    if score != ticket.score {
        return Err(TicketError::Score);
    }
    if score >= target {
        return Err(TicketError::Lost);
    }
    Ok(())
}

/// Prove a winning ticket's marginals.
pub fn prove_ticket(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
    ticket: &SettlementTicket,
) -> Result<ProvenTicket, TicketError> {
    check_statement(&inputs, beacon, cluster, target, ticket)?;
    let relation = TicketRelation::new(inputs, beacon, cluster, ticket);
    let proof = proof::prove(&relation).map_err(TicketError::Proof)?;
    Ok(ProvenTicket {
        ticket: ticket.clone(),
        proof,
    })
}

/// Verify one ticket: statement checks, then the proof of its marginals.
pub fn verify_ticket(
    inputs: TicketInputs<'_>,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
    proven: &ProvenTicket,
) -> Result<(), TicketError> {
    check_statement(&inputs, beacon, cluster, target, &proven.ticket)?;
    let relation = TicketRelation::new(inputs, beacon, cluster, &proven.ticket);
    proof::verify(&relation, &proven.proof).map_err(TicketError::Proof)
}

#[cfg(test)]
#[path = "ticket_proof_tests.rs"]
pub(crate) mod tests;
