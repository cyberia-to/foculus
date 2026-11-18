use super::*;
use crate::ticket_proof::tests::Fixture;
use crate::tickets::{easy_target, grind_fold, self_fold};
use tru::Fx;

fn proof(tickets: Vec<ProvenTicket>) -> ClusterProof {
    ClusterProof {
        target: easy_target(),
        tickets,
    }
}

#[test]
fn honest_cluster_sums_its_tickets() {
    let f = Fixture::new();
    let tickets = f.proven(0xA, 0, 8);
    let acc = verify_cluster(f.inputs(), &f.beacon, &f.cluster, &proof(tickets.clone())).unwrap();
    assert_eq!(acc.k, 8);
    for i in 0..f.contribs.len() {
        let sum = tickets
            .iter()
            .fold(Fx::ZERO, |s, t| s + t.ticket.marginals[i]);
        assert_eq!(acc.sum_m[i], sum);
    }
    let plain: Vec<_> = tickets.iter().map(|t| t.ticket.clone()).collect();
    assert_eq!(acc.commitment, self_fold(f.contribs.len(), &plain).commitment);
}

#[test]
fn one_bad_ticket_among_many_rejects_the_cluster() {
    let f = Fixture::new();
    let mut tickets = f.proven(0xA, 0, 8);
    let crate::proof::Proof::Public(body) = &mut tickets[5].proof else { panic!("public profile") };
    body.certificate.free[0] = (body.certificate.free[0] + 1) % nebu::field::P;
    let r = verify_cluster(f.inputs(), &f.beacon, &f.cluster, &proof(tickets.clone()));
    assert!(matches!(r, Err(ClusterError::Ticket { index: 5, .. })));
    let (accepted, dropped) = accept_tickets(f.inputs(), &f.beacon, &f.cluster, easy_target(), &tickets);
    assert_eq!(dropped, vec![5]);
    assert_eq!(accepted.tickets.len(), 7);
    verify_cluster(f.inputs(), &f.beacon, &f.cluster, &accepted).unwrap();
}

#[test]
fn duplicates_and_empty_clusters_are_rejected() {
    let f = Fixture::new();
    let mut tickets = f.proven(0xA, 0, 2);
    tickets.push(tickets[0].clone());
    let r = verify_cluster(f.inputs(), &f.beacon, &f.cluster, &proof(tickets.clone()));
    assert_eq!(r.unwrap_err(), ClusterError::Duplicate { index: 2 });
    let (accepted, dropped) = accept_tickets(f.inputs(), &f.beacon, &f.cluster, easy_target(), &tickets);
    assert_eq!((accepted.tickets.len(), dropped), (2, vec![2]));
    let empty = verify_cluster(f.inputs(), &f.beacon, &f.cluster, &proof(vec![]));
    assert_eq!(empty.unwrap_err(), ClusterError::Empty);
}

#[test]
fn peer_self_accumulators_are_redrawn_and_liars_dropped() {
    let f = Fixture::new();
    let honest = self_fold(f.contribs.len(), &f.tickets(0xB, 100, 3));
    let redrawn = redraw_peer(f.inputs(), &f.beacon, &f.cluster, easy_target(), &honest).unwrap();
    assert_eq!(redrawn.len(), 3);
    let mut inflated = honest.clone();
    inflated.sum_m[0] = inflated.sum_m[0] + Fx::ONE;
    let r = redraw_peer(f.inputs(), &f.beacon, &f.cluster, easy_target(), &inflated);
    assert_eq!(r.unwrap_err(), ClusterError::Mismatch);
    // a nonce that loses under the target is not a ticket
    let lost = redraw_peer(f.inputs(), &f.beacon, &f.cluster, 0, &honest);
    assert!(matches!(lost, Err(ClusterError::Ticket { .. })));
}

#[test]
fn fold_lottery_step_is_recomputed() {
    let f = Fixture::new();
    let a = self_fold(f.contribs.len(), &f.tickets(0xA, 0, 2));
    let b = self_fold(f.contribs.len(), &f.tickets(0xB, 50, 2));
    let mut ft = grind_fold(&f.beacon, &f.cluster, &[0xF; 32], 0, &a, &b, 0, 16, easy_target()).unwrap();
    assert!(verify_fold_ticket(&f.beacon, &f.cluster, easy_target(), &ft));
    ft.result.sum_m[0] = ft.result.sum_m[0] + Fx::ONE;
    assert!(!verify_fold_ticket(&f.beacon, &f.cluster, easy_target(), &ft));
}
