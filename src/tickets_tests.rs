use super::*;
use tru::Link;

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

fn contribs() -> Vec<Contribution> {
    vec![
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
    ]
}

#[test]
fn easy_grind_finds_winners() {
    let c = contribs();
    let cluster = h(0xC1);
    let beacon = h(0xBE);
    let tickets = grind_settlement(
        &base(),
        &c,
        &Context::none(),
        &FocusingParams::default(),
        &beacon,
        &cluster,
        &h(0x91),
        0,
        32,
        4,
        easy_target(),
    );
    assert_eq!(tickets.len(), 4);
    for t in &tickets {
        assert!(verify_settlement_ticket(t, &beacon, &cluster, easy_target()));
    }
}

#[test]
fn self_fold_counts_samples() {
    let c = contribs();
    let cluster = h(0xC1);
    let beacon = h(0xBE);
    let tickets = grind_settlement(
        &base(),
        &c,
        &Context::none(),
        &FocusingParams::default(),
        &beacon,
        &cluster,
        &h(0x91),
        0,
        16,
        3,
        easy_target(),
    );
    let acc = self_fold(c.len(), &tickets);
    assert_eq!(acc.k, 3);
    assert_eq!(acc.seen.len(), 3);
    // absorb duplicate is no-op
    let mut acc2 = acc.clone();
    absorb_ticket(&mut acc2, &tickets[0]);
    assert_eq!(acc2.k, 3);
}

#[test]
fn fold_acc_is_commutative() {
    let c = contribs();
    let cluster = h(0xC1);
    let beacon = h(0xBE);
    let a_tickets = grind_settlement(
        &base(),
        &c,
        &Context::none(),
        &FocusingParams::default(),
        &beacon,
        &cluster,
        &h(0xA),
        0,
        8,
        2,
        easy_target(),
    );
    let b_tickets = grind_settlement(
        &base(),
        &c,
        &Context::none(),
        &FocusingParams::default(),
        &beacon,
        &cluster,
        &h(0xB),
        100,
        8,
        2,
        easy_target(),
    );
    let a = self_fold(c.len(), &a_tickets);
    let b = self_fold(c.len(), &b_tickets);
    let ab = fold_acc(&a, &b);
    let ba = fold_acc(&b, &a);
    assert_eq!(ab.k, ba.k);
    assert_eq!(ab.seen, ba.seen);
    for i in 0..c.len() {
        assert!((ab.sum_m[i].to_f64() - ba.sum_m[i].to_f64()).abs() < 1e-9);
    }
}

#[test]
fn assemble_tree_merges_miners() {
    let c = contribs();
    let cluster = h(0xC1);
    let beacon = h(0xBE);
    let leaves: Vec<_> = [0xA, 0xB, 0xC]
        .iter()
        .map(|&m| {
            let t = grind_settlement(
                &base(),
                &c,
                &Context::none(),
                &FocusingParams::default(),
                &beacon,
                &cluster,
                &h(m),
                m as u64 * 50,
                8,
                2,
                easy_target(),
            );
            self_fold(c.len(), &t)
        })
        .collect();
    let root = assemble_fold_tree(&beacon, &cluster, &h(0xFF), &leaves, easy_target());
    assert!(root.k >= 6);
    let neurons: Vec<_> = c.iter().map(|x| x.neuron).collect();
    let shares = root.mean_shares(&neurons);
    let sum: f64 = shares.iter().map(|(_, s)| s.to_f64()).sum();
    assert!(sum > 0.0);
}

#[test]
fn k_min_hoeffding() {
    // ε=0.1, δ=0.01 → ln(200)/(2*0.01) ≈ 5.3/0.02 ≈ 265
    let k = ClusterAcc::k_min(0.1, 0.01);
    assert!(k > 100 && k < 400);
}
