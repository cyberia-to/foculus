use super::*;
use crate::beacon::GENESIS_PREV;
use crate::rewards::{claim_from_links, settle_epoch};
use crate::tickets::{easy_target, self_fold};

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

fn claims() -> Vec<RewardClaim> {
    vec![
        claim_from_links(h(0xA1), h(10), vec![Link::stake(h(2), h(1), 8000)], 1),
        claim_from_links(h(0xB2), h(11), vec![Link::stake(h(3), h(1), 6000)], 1),
    ]
}

fn policy(miner: u8, start: u64) -> TicketPolicy {
    TicketPolicy {
        want: 4,
        max_attempts: 64,
        start_nonce: start,
        miner: h(miner),
        ..TicketPolicy::default()
    }
}

fn verify(rec: &SettleReceipt, base: &[Link], claims: &[RewardClaim]) -> Result<(), ReceiptError> {
    let (ctx, params) = (Context::none(), FocusingParams::default());
    verify_settlement(
        rec,
        &SettleVerifyInputs { base, claims, ctx: &ctx, params: &params, target: easy_target() },
    )
}

fn receipt(peers: &[ClusterAcc]) -> SettleReceipt {
    let (ctx, params) = (Context::none(), FocusingParams::default());
    settle_with_peer_accs(1, &GENESIS_PREV, &base(), &claims(), &ctx, &params, 1000, &policy(10, 0), peers)
        .unwrap()
}

#[test]
fn ticket_receipt_verifies_from_public_inputs() {
    let rec = receipt(&[]);
    assert_eq!(rec.sample_count, 4);
    verify(&rec, &base(), &claims()).unwrap();
}

#[test]
fn tampered_receipts_are_rejected() {
    let rec = receipt(&[]);
    let mut shares = rec.clone();
    shares.shares[0].amount += 1;
    assert_eq!(verify(&shares, &base(), &claims()), Err(ReceiptError::Malformed));
    let mut dropped = rec.clone();
    dropped.settlement.as_mut().unwrap().tickets.pop();
    assert!(verify(&dropped, &base(), &claims()).is_err());
    let mut total = rec.clone();
    total.directed_total = total.directed_total + tru::Fx::ONE;
    assert_eq!(verify(&total, &base(), &claims()), Err(ReceiptError::DirectedTotal));
    assert_eq!(verify(&rec, &base(), &claims()[..1]), Err(ReceiptError::Claims));
    let mut target = rec;
    target.settlement.as_mut().unwrap().target = 1;
    assert_eq!(verify(&target, &base(), &claims()), Err(ReceiptError::Target));
}

#[test]
fn peers_are_redrawn_and_a_lying_peer_is_dropped() {
    let (ctx, params) = (Context::none(), FocusingParams::default());
    let cr = crate::beacon::claims_root(&[h(0xA1), h(0xB2)]);
    let b_e = open_beacon(1, &GENESIS_PREV, &cr, &[], TEST_OUTER_T).beacon;
    let contribs = contributions_with_rho(&claims());
    let peer = |miner: u8, start: u64| {
        let t = grind_settlement(&base(), &contribs, &ctx, &params, &b_e, &cr, &h(miner), start, 16, 2, easy_target());
        self_fold(contribs.len(), &t)
    };
    let honest = peer(20, 1000);
    let mut liar = peer(21, 2000);
    liar.sum_m[0] = liar.sum_m[0] + tru::Fx::ONE;
    let rec = receipt(&[honest, liar]);
    assert_eq!(rec.sample_count, 6, "4 local + 2 honest peer tickets");
    verify(&rec, &base(), &claims()).unwrap();
}

#[test]
fn monte_carlo_receipt_is_recomputed() {
    let (ctx, params) = (Context::none(), FocusingParams::default());
    let rec = settle_epoch(1, &GENESIS_PREV, &base(), &claims(), &ctx, &params, 16, 1000).unwrap();
    verify(&rec, &base(), &claims()).unwrap();
    let other = vec![Link::stake(h(1), h(3), 50)];
    assert!(verify(&rec, &other, &claims()).is_err());
}
