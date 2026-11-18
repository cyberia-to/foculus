//! Differential: the accumulated profile accepts exactly what native
//! per-ticket verification accepts, and produces the same accumulator.

use super::*;
use crate::cluster::{ClusterProof, verify_cluster};
use crate::ticket_proof::tests::Fixture;
use crate::ticket_proof::prove_ticket;
use crate::tickets::{commit_marginals, easy_target, settle_score};
use tru::Fx;

fn params() -> AccParams {
    AccParams {
        chunk: 4,
        ..AccParams::settlement()
    }
}

fn prove(f: &Fixture, tickets: &[SettlementTicket]) -> Result<AccumulatedCluster, ClusterError> {
    prove_cluster_accumulated(f.inputs(), &f.beacon, &f.cluster, easy_target(), &params(), tickets)
}

fn verify(f: &Fixture, p: &AccumulatedCluster) -> Result<ClusterAcc, (Part, ClusterError)> {
    verify_cluster_accumulated(f.inputs(), &f.beacon, &f.cluster, &params(), p)
}

fn native(f: &Fixture, tickets: &[SettlementTicket]) -> Result<ClusterAcc, ClusterError> {
    let proven = tickets
        .iter()
        .map(|t| prove_ticket(f.inputs(), &f.beacon, &f.cluster, easy_target(), t))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ClusterError::Ticket { index: 0, error })?;
    verify_cluster(
        f.inputs(),
        &f.beacon,
        &f.cluster,
        &ClusterProof {
            target: easy_target(),
            tickets: proven,
        },
    )
}

fn same(a: &ClusterAcc, b: &ClusterAcc) -> bool {
    a.k == b.k && a.sum_m == b.sum_m && a.commitment == b.commitment && a.seen == b.seen
}

#[test]
fn honest_cluster_matches_native() {
    let f = Fixture::new();
    // 10 tickets, chunk 4: three steps, the last one partial
    let tickets = f.tickets(0xA, 0, 10);
    let p = prove(&f, &tickets).unwrap();
    assert_eq!(p.folded.steps.len(), 3);
    let acc = verify(&f, &p).unwrap();
    assert!(same(&acc, &native(&f, &tickets).unwrap()));
    assert_eq!(acc.k, 10);
}

#[test]
fn a_wrong_value_is_caught_natively_not_by_the_fold() {
    let f = Fixture::new();
    let tickets = f.tickets(0xA, 0, 5);
    // a telescoping forgery: shift every value and one marginal so the rows
    // still hold — the relation is satisfied, only impulse catches it
    let mut forged = tickets.clone();
    let first = crate::settlement::ordering(f.contribs.len(), &f.beacon, forged[2].nonce)[0];
    forged[2].marginals[first] = forged[2].marginals[first] + Fx::ONE;
    forged[2].commitment = commit_marginals(&forged[2].marginals);
    forged[2].score = settle_score(&f.beacon, &f.cluster, forged[2].nonce, &forged[2].miner, &forged[2].commitment);
    // a dishonest prover discloses values that telescope the forged marginals
    let mut p = prove(&f, &tickets).unwrap();
    p.tickets = forged.clone();
    for v in &mut p.values[2] {
        *v += Fx::ONE.raw();
    }
    let relations: Vec<_> = p
        .tickets
        .iter()
        .map(|t| TicketRelation::new(f.inputs(), &f.beacon, &f.cluster, t))
        .collect();
    let witnesses: Vec<Vec<F>> = relations.iter().zip(&p.values).map(|(r, v)| r.assignment(v)).collect();
    let derived = Derived::new(&relations, &p.values);
    p.folded = ccs::fold(&params().whir, &derived.statements(), &witnesses, params().chunk).unwrap();
    verify_folded_part(f.inputs(), &f.beacon, &f.cluster, &params(), &p).unwrap();
    assert_eq!(
        verify(&f, &p).unwrap_err(),
        (Part::Native, ClusterError::Ticket { index: 2, error: TicketError::Proof(ProofError::WitnessCheck) })
    );
    assert!(native(&f, &forged).is_err());
}

#[test]
fn tampering_is_rejected_by_the_folded_proof() {
    let f = Fixture::new();
    let tickets = f.tickets(0xA, 0, 6);
    let p = prove(&f, &tickets).unwrap();
    // a disclosed value changed after proving
    let mut bad = p.clone();
    bad.values[4][1] += F::ONE;
    assert_eq!(verify(&f, &bad).unwrap_err(), (Part::Proof, ClusterError::Accumulation));
    // a claim proof, a step, the decider
    let mut bad = p.clone();
    bad.folded.claims[3].witness_eval += nebu::Fp3::ONE;
    assert_eq!(verify(&f, &bad).unwrap_err().0, Part::Proof);
    let mut bad = p.clone();
    bad.folded.steps.pop();
    assert_eq!(verify(&f, &bad).unwrap_err().0, Part::Proof);
    // tickets swapped against their claim proofs
    let mut bad = p.clone();
    bad.tickets.swap(0, 1);
    bad.values.swap(0, 1);
    assert_eq!(verify(&f, &bad).unwrap_err().0, Part::Proof);
    // a ticket dropped
    let mut bad = p.clone();
    bad.tickets.pop();
    bad.values.pop();
    assert_eq!(verify(&f, &bad).unwrap_err().0, Part::Proof);
    // another beacon
    let mut g = Fixture::new();
    g.beacon[1] = 7;
    assert!(verify(&g, &p).is_err());
}

#[test]
fn statement_checks_and_duplicates_match_native() {
    let f = Fixture::new();
    let mut tickets = f.tickets(0xA, 0, 3);
    tickets.push(tickets[0].clone());
    assert_eq!(prove(&f, &tickets).unwrap_err(), ClusterError::Duplicate { index: 3 });
    let p = prove(&f, &tickets[..3]).unwrap();
    let mut bad = p.clone();
    bad.tickets.push(bad.tickets[0].clone());
    bad.values.push(bad.values[0].clone());
    assert_eq!(verify(&f, &bad).unwrap_err(), (Part::Proof, ClusterError::Duplicate { index: 3 }));
    let mut bad = p.clone();
    bad.tickets[1].score ^= 1;
    assert_eq!(
        verify(&f, &bad).unwrap_err(),
        (Part::Proof, ClusterError::Ticket { index: 1, error: TicketError::Score })
    );
    assert_eq!(prove(&f, &[]).unwrap_err(), ClusterError::Empty);
}
