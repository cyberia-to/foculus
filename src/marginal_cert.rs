// ---
// tags: foculus, rust, rewards, Δφ⁺, tickets, zheng
// crystal-type: source
// crystal-domain: cyber
// ---
//! Ticket proofs that certify Δφ⁺ / Shapley marginals.
//!
//! Spec (fold-mining): each winning ticket is `(n, m(n), σ)` where `m(n)` is
//! the vector of per-contributor marginals under beacon ordering π(n), and
//! each marginal is `v★(prefix∪{i}) − v★(prefix)` with `v★ = Δφ⁺(ρ-weighted)`.
//!
//! Certification:
//! 1. **Replay**: recompute `ordering` + `marginals` from public (base, contribs,
//!    beacon, nonce) and require `commit(m) == ticket.commitment`.
//! 2. **HyperNova σ**: fold commitment + each m[i] raw into ticket accumulator;
//!    decide binds statement to (beacon, cluster, commit(m), k).
//!
//! Anyone with the public claim set can verify without trusting the miner.

use tru::{Context, FocusingParams, Fx, Link};

use crate::settlement::{self, Contribution};
use crate::ticket_proof::{
    prove_settlement_batch, verify_fold_seal, FoldSeal, ProofError, TicketProver,
};
use crate::tickets::{
    commit_marginals, settle_score, ClusterId, SettlementTicket,
};

const MARGINAL_PROG: [u8; 32] = *b"foculus-marginal-cert-v1\0\0\0\0\0\0\0\0";

/// A ticket whose marginals were recomputed and sealed under HyperNova.
#[derive(Clone, Debug)]
pub struct CertifiedTicket {
    pub ticket: SettlementTicket,
    /// Seal covering this ticket's commitment + marginal raws.
    pub seal: FoldSeal,
    /// Ordering π(n) used (public).
    pub order: Vec<usize>,
}

/// Replay Δφ⁺ marginals for a ticket; returns recomputed vector if commitment matches.
pub fn replay_marginals(
    base: &[Link],
    contribs: &[Contribution],
    ctx: &Context,
    params: &FocusingParams,
    beacon: &[u8; 32],
    ticket: &SettlementTicket,
) -> Option<Vec<Fx>> {
    let n = contribs.len();
    if n == 0 || ticket.marginals.len() != n {
        return None;
    }
    let order = settlement::ordering(n, beacon, ticket.nonce);
    let m = settlement::marginals(base, contribs, &order, ctx, params);
    if commit_marginals(&m) != ticket.commitment {
        return None;
    }
    // Exact match of published marginals (field equality via raw).
    for (a, b) in m.iter().zip(ticket.marginals.iter()) {
        if a.raw().as_u64() != b.raw().as_u64() {
            return None;
        }
    }
    Some(m)
}

/// Certify a winning ticket: replay Δφ⁺ marginals + HyperNova seal on commitment.
pub fn certify_ticket(
    base: &[Link],
    contribs: &[Contribution],
    ctx: &Context,
    params: &FocusingParams,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    ticket: &SettlementTicket,
) -> Result<CertifiedTicket, CertError> {
    let order = settlement::ordering(contribs.len(), beacon, ticket.nonce);
    let _m = replay_marginals(base, contribs, ctx, params, beacon, ticket)
        .ok_or(CertError::ReplayFailed)?;
    let mut prover = TicketProver::new();
    prover
        .fold_settlement(beacon, cluster, ticket)
        .map_err(CertError::Proof)?;
    let seal = prover.seal(beacon, cluster, 1).map_err(CertError::Proof)?;
    if !verify_fold_seal(&seal) {
        return Err(CertError::Proof(ProofError::VerifyFailed));
    }
    Ok(CertifiedTicket {
        ticket: ticket.clone(),
        seal,
        order,
    })
}

/// Verify certified ticket against public graph inputs.
///
/// This is the claim: σ certifies Δφ⁺/Shapley marginals — because verification
/// **replays** impulse-based marginals and checks the HyperNova seal.
pub fn verify_certified_ticket(
    base: &[Link],
    contribs: &[Contribution],
    ctx: &Context,
    params: &FocusingParams,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    target: u64,
    cert: &CertifiedTicket,
) -> bool {
    // Win-test
    let score = settle_score(
        beacon,
        cluster,
        cert.ticket.nonce,
        &cert.ticket.miner,
        &cert.ticket.commitment,
    );
    if score != cert.ticket.score || score >= target {
        return false;
    }
    // Replay Δφ⁺ marginals
    if replay_marginals(base, contribs, ctx, params, beacon, &cert.ticket).is_none() {
        return false;
    }
    // HyperNova seal
    if !verify_fold_seal(&cert.seal) {
        return false;
    }
    // Ordering consistency
    let order = settlement::ordering(contribs.len(), beacon, cert.ticket.nonce);
    order == cert.order
}

/// Certify a batch of winning tickets (each independently replayed).
pub fn certify_batch(
    base: &[Link],
    contribs: &[Contribution],
    ctx: &Context,
    params: &FocusingParams,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    tickets: &[SettlementTicket],
) -> Result<Vec<CertifiedTicket>, CertError> {
    tickets
        .iter()
        .map(|t| certify_ticket(base, contribs, ctx, params, beacon, cluster, t))
        .collect()
}

/// Batch HyperNova over tickets that already pass replay (faster seal path).
pub fn prove_replayed_batch(
    base: &[Link],
    contribs: &[Contribution],
    ctx: &Context,
    params: &FocusingParams,
    beacon: &[u8; 32],
    cluster: &ClusterId,
    tickets: &[SettlementTicket],
) -> Result<FoldSeal, CertError> {
    for t in tickets {
        if replay_marginals(base, contribs, ctx, params, beacon, t).is_none() {
            return Err(CertError::ReplayFailed);
        }
    }
    prove_settlement_batch(beacon, cluster, tickets).map_err(CertError::Proof)
}

#[derive(Debug, PartialEq, Eq)]
pub enum CertError {
    ReplayFailed,
    Proof(ProofError),
}

/// Domain tag for statements that bind marginal certification.
pub fn marginal_program_hash() -> [u8; 32] {
    MARGINAL_PROG
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tickets::{easy_target, grind_settlement};
    use tru::Link;

    fn h(b: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        x[0] = b;
        x
    }

    fn setup() -> (Vec<Link>, Vec<Contribution>, [u8; 32], [u8; 32]) {
        let base = vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ];
        let contribs = vec![
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
        ];
        (base, contribs, h(0xBE), h(0xC1))
    }

    #[test]
    fn certify_and_verify_replays_delta_phi() {
        let (base, contribs, beacon, cluster) = setup();
        let tickets = grind_settlement(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &h(0x91),
            0,
            32,
            2,
            easy_target(),
        );
        assert!(!tickets.is_empty());
        let cert = certify_ticket(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &tickets[0],
        )
        .unwrap();
        assert!(verify_certified_ticket(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            easy_target(),
            &cert,
        ));
        // Marginal sum is directed coalition value along that order (non-negative total)
        let sum: f64 = cert.ticket.marginals.iter().map(|m| m.to_f64()).sum();
        assert!(sum >= 0.0);
    }

    #[test]
    fn tampered_marginal_fails_replay() {
        let (base, contribs, beacon, cluster) = setup();
        let mut tickets = grind_settlement(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &h(0x91),
            0,
            16,
            1,
            easy_target(),
        );
        tickets[0].marginals[0] = Fx::ONE; // break commitment
        assert!(replay_marginals(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &tickets[0],
        )
        .is_none());
    }

    #[test]
    fn batch_prove_requires_replay() {
        let (base, contribs, beacon, cluster) = setup();
        let tickets = grind_settlement(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &h(0x91),
            0,
            32,
            3,
            easy_target(),
        );
        let seal = prove_replayed_batch(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &tickets,
        )
        .unwrap();
        assert!(verify_fold_seal(&seal));
    }
}
