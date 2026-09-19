//! Property #4 — progress-freedom across clusters (specs/fold-mining.md
//! "settlement difficulty"): a large cluster's settlement attempts must not
//! be systematically starved by a flat difficulty target.
//!
//! `settlement::marginals` costs one `tru::impulse` call per contributor, so
//! a flat target gives a large cluster the same win probability per attempt
//! as a small one but a much lower win probability per unit of wall-clock
//! time. This test measures the real per-attempt cost at two cluster sizes
//! and checks that `tickets::banded_target` equalizes expected time-to-win
//! where a flat target does not.

use foculus::settlement::Contribution;
use foculus::tickets::{banded_target, easy_target, try_settlement_ticket};
use std::time::Instant;
use tru::{Context, FocusingParams, Fx, Link};

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

/// `n` contributors, each with one link — a coalition of size `n`.
fn cluster(n: usize) -> Vec<Contribution> {
    (0..n)
        .map(|i| Contribution {
            neuron: h((16 + i) as u8),
            links: vec![Link::stake(h(2), h(1), 100 + i as u128)],
            surprise: Fx::ONE,
        })
        .collect()
}

/// Average wall-clock cost of one settlement attempt (`try_settlement_ticket`
/// with an always-losing target, so no early exit skews the timing) over
/// `iters` nonces.
fn attempt_cost_secs(contribs: &[Contribution], iters: u64) -> f64 {
    let base = base();
    let ctx = Context::none();
    let params = FocusingParams::default();
    let beacon = h(0xBE);
    let cluster_id = h(0xC1);
    let miner = h(0x91);
    // target 0 never wins, so every attempt pays full marginals cost.
    let start = Instant::now();
    for nonce in 0..iters {
        let _ = try_settlement_ticket(
            &base, contribs, &ctx, &params, &beacon, &cluster_id, &miner, nonce, 0,
        );
    }
    start.elapsed().as_secs_f64() / iters as f64
}

#[test]
fn banded_target_equalizes_expected_time_to_win_flat_target_does_not() {
    let small = cluster(4);
    let large = cluster(32);

    let cost_small = attempt_cost_secs(&small, 40);
    let cost_large = attempt_cost_secs(&large, 40);
    assert!(
        cost_large > cost_small,
        "large cluster attempt ({cost_large}s) should cost more than small ({cost_small}s) \
         — otherwise this test isn't exercising the property it claims to"
    );

    let win_prob = |target: u64| target as f64 / (u64::MAX as f64 + 1.0);
    let expected_time = |cost: f64, target: u64| cost / win_prob(target);

    let flat = easy_target() >> 8; // a real, non-trivial flat target
    let flat_time_small = expected_time(cost_small, flat);
    let flat_time_large = expected_time(cost_large, flat);
    let flat_ratio = flat_time_large / flat_time_small;
    // Flat target: expected time scales with attempt cost, i.e. with cluster
    // size — the large cluster is starved roughly n_large/n_small-fold.
    assert!(
        flat_ratio > 4.0,
        "flat target expected to starve the large cluster (ratio {flat_ratio:.2}, want > 4)"
    );

    let banded_small = banded_target(flat, small.len(), small.len());
    let banded_large = banded_target(flat, large.len(), small.len());
    let banded_time_small = expected_time(cost_small, banded_small);
    let banded_time_large = expected_time(cost_large, banded_large);
    let banded_ratio = banded_time_large / banded_time_small;
    eprintln!(
        "[progress-freedom] cost small(n=4)={cost_small:.6}s large(n=32)={cost_large:.6}s \
         flat_ratio={flat_ratio:.2} banded_ratio={banded_ratio:.2}"
    );
    // Banded target: expected time-to-win is within a small constant factor
    // of equal across cluster sizes — progress-freedom.
    assert!(
        (0.5..2.0).contains(&banded_ratio),
        "banded target should equalize expected time to win (ratio {banded_ratio:.2}, want in [0.5, 2.0])"
    );
}
