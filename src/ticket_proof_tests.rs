use super::*;
use crate::tickets::{easy_target, grind_settlement};
use tru::Fx;
use zheng::execution::certificate;

fn h(b: u8) -> [u8; 32] {
    let mut x = [0u8; 32];
    x[0] = b;
    x
}

pub(crate) struct Fixture {
    pub base: Vec<Link>,
    pub contribs: Vec<Contribution>,
    pub ctx: Context,
    pub params: FocusingParams,
    pub beacon: [u8; 32],
    pub cluster: [u8; 32],
}

impl Fixture {
    pub fn new() -> Self {
        let c = |n: u8, from: u8, to: u8, w: u128| Contribution {
            neuron: h(n),
            links: vec![Link::stake(h(from), h(to), w)],
            surprise: Fx::ONE,
        };
        Self {
            base: vec![
                Link::stake(h(1), h(2), 100),
                Link::stake(h(2), h(3), 100),
                Link::stake(h(3), h(1), 100),
            ],
            contribs: vec![c(10, 2, 1, 8000), c(11, 3, 1, 6000), c(12, 1, 3, 4000)],
            ctx: Context::none(),
            params: FocusingParams::default(),
            beacon: h(0xBE),
            cluster: h(0xC1),
        }
    }
    pub fn inputs(&self) -> TicketInputs<'_> {
        TicketInputs {
            base: &self.base,
            contribs: &self.contribs,
            ctx: &self.ctx,
            params: &self.params,
        }
    }
    pub fn tickets(&self, miner: u8, start: u64, want: usize) -> Vec<SettlementTicket> {
        grind_settlement(
            &self.base,
            &self.contribs,
            &self.ctx,
            &self.params,
            &self.beacon,
            &self.cluster,
            &h(miner),
            start,
            want as u64 * 4,
            want,
            easy_target(),
        )
    }
    pub fn proven(&self, miner: u8, start: u64, want: usize) -> Vec<ProvenTicket> {
        self.tickets(miner, start, want)
            .iter()
            .map(|t| prove_ticket(self.inputs(), &self.beacon, &self.cluster, easy_target(), t).unwrap())
            .collect()
    }
    pub fn verify(&self, p: &ProvenTicket) -> Result<(), TicketError> {
        verify_ticket(self.inputs(), &self.beacon, &self.cluster, easy_target(), p)
    }
}

/// Re-commit and re-score a ticket after its marginals or miner changed, so
/// only the proof can catch the change.
fn restamp(f: &Fixture, t: &mut SettlementTicket) {
    t.commitment = commit_marginals(&t.marginals);
    t.score = settle_score(&f.beacon, &f.cluster, t.nonce, &t.miner, &t.commitment);
}

/// A certificate for `ticket` whose coalition values telescope to its
/// (possibly forged) marginals — satisfies the CCS by construction.
fn telescoping_proof(f: &Fixture, ticket: &SettlementTicket) -> Proof {
    let relation = TicketRelation::new(f.inputs(), &f.beacon, &f.cluster, ticket);
    let mut z = vec![F::ZERO; relation.instance().num_cols];
    z[0] = F::ONE;
    for (col, v) in relation.public() {
        z[col] = v;
    }
    let mut acc = F::ZERO;
    for (j, &i) in relation.order.iter().enumerate() {
        acc += ticket.marginals[i].raw();
        z[relation.v_col(j)] = acc;
    }
    let cert = certificate::certify(&relation.instance(), &CCSWitness { z }, &relation.public())
        .expect("telescoping witness satisfies the rows");
    Proof::Public(crate::proof::PublicProof {
        statement: relation.statement(),
        certificate: cert,
    })
}

#[test]
fn honest_ticket_verifies() {
    let f = Fixture::new();
    for p in f.proven(0x91, 0, 4) {
        f.verify(&p).unwrap();
        assert_eq!(p.proof.statement(), &TicketRelation::new(f.inputs(), &f.beacon, &f.cluster, &p.ticket).statement());
    }
}

#[test]
fn wrong_marginal_cannot_be_proven_or_forged() {
    let f = Fixture::new();
    let mut t = f.tickets(0x91, 0, 1).remove(0);
    t.marginals[0] = t.marginals[0] + Fx::ONE;
    t.marginals[1] = t.marginals[1] - Fx::ONE; // same total, wrong split
    restamp(&f, &mut t);
    assert!(prove_ticket(f.inputs(), &f.beacon, &f.cluster, easy_target(), &t).is_err());
    let forged = ProvenTicket { proof: telescoping_proof(&f, &t), ticket: t };
    assert_eq!(f.verify(&forged), Err(TicketError::Proof(ProofError::WitnessCheck)));
}

#[test]
fn marginals_of_another_ordering_are_rejected() {
    let f = Fixture::new();
    let t = f.tickets(0x91, 0, 1).remove(0);
    let n = f.contribs.len();
    let own = settlement::ordering(n, &f.beacon, t.nonce);
    let other = (1..64)
        .map(|k| settlement::ordering(n, &f.beacon, t.nonce + k))
        .find(|o| *o != own)
        .unwrap();
    let mut forged = t.clone();
    forged.marginals = settlement::marginals(&f.base, &f.contribs, &other, &f.ctx, &f.params);
    assert_ne!(forged.marginals, t.marginals, "fixture must make order matter");
    restamp(&f, &mut forged);
    let proof = telescoping_proof(&f, &forged);
    assert!(f.verify(&ProvenTicket { ticket: forged, proof }).is_err());
}

#[test]
fn wrong_beacon_is_rejected() {
    let f = Fixture::new();
    let p = f.proven(0x91, 0, 1).remove(0);
    let other = TicketInputs { ..f.inputs() };
    let r = verify_ticket(other, &h(0xEE), &f.cluster, easy_target(), &p);
    assert_eq!(r, Err(TicketError::Score));
}

#[test]
fn wrong_identity_is_rejected() {
    let f = Fixture::new();
    let mut p = f.proven(0x91, 0, 1).remove(0);
    p.ticket.miner = h(0x92);
    assert_eq!(f.verify(&p), Err(TicketError::Score));
    // re-scored for the new miner: the proof is bound to the old identity
    restamp(&f, &mut p.ticket);
    assert_eq!(f.verify(&p), Err(TicketError::Proof(ProofError::WrongStatement)));
}

#[test]
fn certificate_of_another_ticket_is_rejected() {
    let f = Fixture::new();
    let mut ps = f.proven(0x91, 0, 2);
    let other = ps[1].proof.clone();
    ps[0].proof = other;
    assert_eq!(f.verify(&ps[0]), Err(TicketError::Proof(ProofError::WrongStatement)));
}

#[test]
fn another_base_graph_is_rejected() {
    let f = Fixture::new();
    let p = f.proven(0x91, 0, 1).remove(0);
    let base = vec![Link::stake(h(1), h(3), 50)];
    let inputs = TicketInputs { base: &base, ..f.inputs() };
    let r = verify_ticket(inputs, &f.beacon, &f.cluster, easy_target(), &p);
    assert_eq!(r, Err(TicketError::Proof(ProofError::WitnessCheck)));
}

#[test]
fn losing_and_malformed_tickets_are_rejected() {
    let f = Fixture::new();
    let p = f.proven(0x91, 0, 1).remove(0);
    let lost = verify_ticket(f.inputs(), &f.beacon, &f.cluster, p.ticket.score, &p);
    assert_eq!(lost, Err(TicketError::Lost));
    let mut short = p.clone();
    short.ticket.marginals.pop();
    assert_eq!(f.verify(&short), Err(TicketError::Shape));
    let mut uncommitted = p;
    uncommitted.ticket.commitment[0] ^= 1;
    assert_eq!(f.verify(&uncommitted), Err(TicketError::Commitment));
}
