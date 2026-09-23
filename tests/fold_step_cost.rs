//! Property #5 / #3 — "fold is a commutative monoid, decide is O(1)"
//! (tru/specs/rewards.md §7; specs/fold-mining.md "the fold step"); this
//! measures the claim in docs/explanation/latency-targets.md's clock-C
//! table: "steady-state per tip | O(1) fold (~30 field ops class)".
//!
//! `absorb_ticket` and `fold_acc` (src/tickets.rs) both touch the
//! accumulator's `seen: BTreeSet<(miner, nonce)>` and rehash the full
//! accumulator via `acc_commitment` on every call — real costs the
//! "~30 field ops" estimate does not name. `k` is bounded by
//! `ClusterAcc::k_min` (Hoeffding target, ~265 for ε=0.1, δ=0.01;
//! `k_min_hoeffding` in src/tickets.rs), so this is a bounded, not
//! unbounded, cost — but "bounded by a few hundred BTreeSet ops + one
//! hash" and "~30 field ops" are different claims. This measures the
//! real one at realistic k.

use std::time::Instant;

use foculus::settlement::Contribution;
use foculus::tickets::{absorb_ticket, fold_acc, grind_settlement, ClusterAcc};
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

/// 8 contributors, matching foculus/audit/hardware-profile.md's ε-support range.
fn cluster(n: usize) -> Vec<Contribution> {
    (0..n)
        .map(|i| Contribution {
            neuron: h((16 + i) as u8),
            links: vec![Link::stake(h(2), h(1), 100 + i as u128 * 900)],
            surprise: Fx::ONE,
        })
        .collect()
}

#[test]
fn absorb_ticket_cost_grows_with_accumulator_size() {
    let n_contrib = 8;
    let k_min = ClusterAcc::k_min(0.1, 0.01) as usize; // ~265
    let pool = grind_settlement(
        &base(),
        &cluster(n_contrib),
        &Context::none(),
        &FocusingParams::default(),
        &h(0xBE),
        &h(0xC1),
        &h(0x01),
        0,
        (k_min as u64 + 8) * 4,
        k_min + 8,
        u64::MAX, // easy_target(): every nonce wins
    );
    assert!(
        pool.len() >= k_min + 8,
        "need k_min + 8 real tickets, got {}",
        pool.len()
    );

    // Cold: one absorb into an empty accumulator (k: 0 -> 1).
    let iters = 2000u32;
    let start = Instant::now();
    for _ in 0..iters {
        let mut acc = ClusterAcc::empty(n_contrib);
        absorb_ticket(&mut acc, &pool[0]);
    }
    let cold_ns = start.elapsed().as_nanos() as f64 / iters as f64;

    // Steady-state: one more absorb into an accumulator already holding
    // k_min - 1 tickets (k: k_min-1 -> k_min), the realistic "per new
    // block" case a light client repeats every tip.
    let mut warm = ClusterAcc::empty(n_contrib);
    for t in &pool[..k_min - 1] {
        absorb_ticket(&mut warm, t);
    }
    assert_eq!(warm.k as usize, k_min - 1);
    let extra = &pool[k_min - 1..k_min + 7]; // 8 more distinct tickets to absorb repeatedly
    let start = Instant::now();
    for i in 0..iters {
        let mut probe = warm.clone();
        absorb_ticket(&mut probe, &extra[i as usize % extra.len()]);
    }
    let warm_ns = start.elapsed().as_nanos() as f64 / iters as f64;

    eprintln!(
        "[fold-step-cost] absorb_ticket: cold(k=0->1)={cold_ns:.0} ns/call, steady(k={}->{})={warm_ns:.0} ns/call, ratio={:.2}x",
        k_min - 1,
        k_min,
        warm_ns / cold_ns.max(1.0)
    );

    assert!(cold_ns > 0.0 && warm_ns > 0.0);
    // Real finding, not an assumption: absorb_ticket rehashes the whole
    // accumulator (acc_commitment) and touches the whole `seen` BTreeSet
    // on every call, so cost at k=k_min must exceed cost at k=1 by more
    // than noise — if this ever stops holding, the O(1)-vs-O(k) claim in
    // this file's header comment needs re-checking, not just the numbers.
    assert!(
        warm_ns > cold_ns,
        "expected steady-state absorb (k={}) to cost more than cold absorb (k=1); \
         got cold={cold_ns:.0}ns warm={warm_ns:.0}ns — the O(k) cost this file \
         documents may have changed",
        k_min
    );
}

#[test]
fn fold_acc_cost_at_k_min_sized_accumulators() {
    let n_contrib = 8;
    let k_min = ClusterAcc::k_min(0.1, 0.01) as usize; // ~265
    let half = k_min / 2;

    let left_pool = grind_settlement(
        &base(),
        &cluster(n_contrib),
        &Context::none(),
        &FocusingParams::default(),
        &h(0xBE),
        &h(0xC1),
        &h(0x01),
        0,
        (half as u64 + 1) * 4,
        half,
        u64::MAX,
    );
    let right_pool = grind_settlement(
        &base(),
        &cluster(n_contrib),
        &Context::none(),
        &FocusingParams::default(),
        &h(0xBE),
        &h(0xC1),
        &h(0x02), // distinct miner -> distinct (miner, nonce) pairs, no seen-overlap
        0,
        (half as u64 + 1) * 4,
        half,
        u64::MAX,
    );
    assert!(left_pool.len() >= half && right_pool.len() >= half);

    let mut left = ClusterAcc::empty(n_contrib);
    for t in &left_pool[..half] {
        absorb_ticket(&mut left, t);
    }
    let mut right = ClusterAcc::empty(n_contrib);
    for t in &right_pool[..half] {
        absorb_ticket(&mut right, t);
    }
    assert_eq!(left.k as usize + right.k as usize, half * 2);

    let iters = 2000u32;
    let start = Instant::now();
    for _ in 0..iters {
        let _ = fold_acc(&left, &right);
    }
    let fold_ns = start.elapsed().as_nanos() as f64 / iters as f64;

    eprintln!(
        "[fold-step-cost] fold_acc: two k={half} accumulators (k_min={k_min}) -> {fold_ns:.0} ns/call"
    );
    assert!(fold_ns > 0.0);
}
