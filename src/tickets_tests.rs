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
fn role_separation_rejects_a_contending_miner() {
    let c = contribs();
    assert!(!role_separation_holds(&h(10), &c)); // h(10) is contribs()[0].neuron
    assert!(!role_separation_holds(&h(11), &c)); // contribs()[1].neuron
    assert!(role_separation_holds(&h(0x91), &c)); // not a contender
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
fn fold_acc_overlap_doubles_the_mean_a_monoid_law_violation() {
    // Documents the existing gap in `fold_acc`: folding an accumulator
    // with itself should be a no-op under a real monoid (it counts the
    // same (id, n) pairs), but `sum_m` is re-summed unconditionally on
    // overlap, so the mean silently doubles.
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
    let folded_with_self = fold_acc(&acc, &acc);
    assert_eq!(folded_with_self.k, acc.k, "k is unaffected by the bug");
    for i in 0..c.len() {
        let doubled = acc.sum_m[i] + acc.sum_m[i];
        assert!(
            (folded_with_self.sum_m[i].to_f64() - doubled.to_f64()).abs() < 1e-9,
            "fold_acc(x, x) should reproduce the doubling bug this test documents"
        );
    }
}

#[test]
fn fold_acc_checked_rejects_overlap_instead_of_doubling() {
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
    assert_eq!(
        fold_acc_checked(&acc, &acc).unwrap_err(),
        FoldOverlap::NonDisjoint(acc.seen.len())
    );

    // A partial overlap (one shared ticket) is rejected too.
    let (first, rest) = tickets.split_first().unwrap();
    let left = self_fold(c.len(), std::slice::from_ref(first));
    let right = self_fold(c.len(), tickets.as_slice());
    assert_eq!(
        fold_acc_checked(&left, &right).unwrap_err(),
        FoldOverlap::NonDisjoint(1)
    );
    let _ = rest;
}

#[test]
fn fold_acc_checked_matches_fold_acc_when_disjoint() {
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
    let via_unchecked = fold_acc(&a, &b);
    let via_checked = fold_acc_checked(&a, &b).expect("disjoint accumulators fold cleanly");
    assert_eq!(via_unchecked.k, via_checked.k);
    assert_eq!(via_unchecked.seen, via_checked.seen);
    for i in 0..c.len() {
        assert!(
            (via_unchecked.sum_m[i].to_f64() - via_checked.sum_m[i].to_f64()).abs() < 1e-9
        );
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
    let k = ClusterAcc::k_min(Fx::from_ratio(1, 10), Fx::from_ratio(1, 100));
    assert!(k > 100 && k < 400);
}

#[test]
fn k_min_degenerate_inputs_return_one() {
    assert_eq!(ClusterAcc::k_min(Fx::ZERO, Fx::from_ratio(1, 100)), 1);
    assert_eq!(ClusterAcc::k_min(Fx::from_ratio(1, 10), Fx::ZERO), 1);
    assert_eq!(ClusterAcc::k_min(Fx::from_ratio(1, 10), Fx::ONE), 1);
}

#[test]
fn k_min_matches_f64_hoeffding_within_rounding() {
    // Cross-check the fixed-point k_min against the textbook f64 formula
    // at several (ε, δ) pairs — same ceil(ln(2/δ)/(2ε²)), off by at most
    // one unit near a boundary from fixed-point rounding.
    for (eps_num, eps_den, delta_num, delta_den) in
        [(1, 10, 1, 100), (1, 20, 1, 1000), (1, 4, 1, 10), (3, 100, 1, 50)]
    {
        let epsilon = eps_num as f64 / eps_den as f64;
        let delta = delta_num as f64 / delta_den as f64;
        let want = ((2.0 / delta).ln() / (2.0 * epsilon * epsilon)).ceil() as i64;
        let got = ClusterAcc::k_min(
            Fx::from_ratio(eps_num, eps_den),
            Fx::from_ratio(delta_num, delta_den),
        ) as i64;
        assert!(
            (got - want).abs() <= 1,
            "ε={epsilon} δ={delta}: fixed-point k_min={got} vs f64={want}"
        );
    }
}

#[test]
fn banded_target_scales_linearly_with_cluster_size() {
    let base = 1u64 << 32;
    assert_eq!(banded_target(base, 8, 8), base);
    assert_eq!(banded_target(base, 16, 8), base * 2);
    assert_eq!(banded_target(base, 4, 8), base / 2);
}

#[test]
fn banded_target_saturates_instead_of_overflowing() {
    let t = banded_target(u64::MAX / 2, 1_000_000, 1);
    assert_eq!(t, u64::MAX);
}

#[test]
fn banded_target_floors_zero_sizes_to_one() {
    // n_contrib=0 or base_n=0 must not divide by zero or panic.
    assert_eq!(banded_target(1024, 0, 8), 128);
    assert_eq!(banded_target(1024, 8, 0), 1024 * 8);
}

#[test]
fn withholding_bias_bound_matches_measured_shape() {
    // foculus/audit/withholding-bias.md: q=0.05 bound≈0.000106,
    // q=0.50 bound≈0.002012, range=mean-min from that run ≈ 0.002012.
    let range = Fx::from_ratio(2012, 1_000_000);
    let bound_05 = withholding_bias_bound(range, Fx::from_ratio(5, 100));
    let bound_50 = withholding_bias_bound(range, Fx::from_ratio(50, 100));
    assert!(bound_05 > Fx::ZERO);
    assert!(
        bound_50 > bound_05,
        "bound must grow with compute share q, per (mean-min)*q/(1-q)"
    );
}

#[test]
fn withholding_bias_bound_is_zero_at_zero_share() {
    let range = Fx::from_ratio(2012, 1_000_000);
    assert_eq!(withholding_bias_bound(range, Fx::ZERO), Fx::ZERO);
}

#[test]
fn forfeit_below_bound_does_not_deter() {
    let range = Fx::from_ratio(2012, 1_000_000);
    let q = Fx::from_ratio(25, 100);
    let cluster_value = Fx::from_int(1_000_000);
    let bound_value = withholding_bias_bound(range, q) * cluster_value;
    let underpriced = bound_value - Fx::from_int(1);
    assert!(!forfeit_deters_withholding(
        underpriced,
        range,
        q,
        cluster_value
    ));
}

#[test]
fn forfeit_above_bound_deters() {
    let range = Fx::from_ratio(2012, 1_000_000);
    let q = Fx::from_ratio(25, 100);
    let cluster_value = Fx::from_int(1_000_000);
    let bound_value = withholding_bias_bound(range, q) * cluster_value;
    let overpriced = bound_value + Fx::from_int(1);
    assert!(forfeit_deters_withholding(
        overpriced,
        range,
        q,
        cluster_value
    ));
}
