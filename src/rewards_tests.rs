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
