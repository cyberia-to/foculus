//! Measures that `absorb_ticket` / `fold_acc` stay near constant-time at
//! steady state, closing the gap named in foculus/audit/fold-step-cost.md:
//! `acc_commitment` used to rehash the whole `seen` set on every call, so
//! steady-state `absorb_ticket` cost scaled with k (measured ~656us at
//! k=264->265, 61x its cold-start cost). `seen` now carries an incrementally
//! maintained digest (tickets::ClusterAcc::seen_digest), so `acc_commitment`
//! hashes a small fixed-size buffer regardless of k.

use foculus::tickets::{absorb_ticket, fold_acc, ClusterAcc, SettlementTicket};
use std::time::Instant;
use tru::Fx;

fn ticket(i: u64, n: usize) -> SettlementTicket {
    let mut miner = [0u8; 32];
    miner[0..8].copy_from_slice(&i.to_le_bytes());
    SettlementTicket {
        miner,
        nonce: i,
        marginals: vec![Fx::ONE; n],
        commitment: [0u8; 32],
        score: 0,
    }
}

/// Average per-call cost of absorbing `count` fresh tickets starting at k=`from`.
/// Averaging over a batch (rather than timing one call) smooths out one-shot
/// timer/allocator noise that a single-call measurement is too flaky to survive.
fn avg_absorb_ns(acc: &mut ClusterAcc, from: u64, count: u64, n: usize) -> f64 {
    let t = Instant::now();
    for i in from..from + count {
        absorb_ticket(acc, &ticket(i, n));
    }
    t.elapsed().as_nanos() as f64 / count as f64
}

#[test]
fn absorb_ticket_steady_state_stays_near_early_state() {
    let n = 8;
    let mut acc = ClusterAcc::empty(n);

    let early_ns = avg_absorb_ns(&mut acc, 0, 50, n);
    assert_eq!(acc.k, 50);
    let steady_ns = avg_absorb_ns(&mut acc, 50, 500, n);
    assert_eq!(acc.k, 550);

    println!(
        "[incremental-acc] absorb_ticket avg ns/call: k0..50={early_ns:.0} k50..550={steady_ns:.0} ratio={:.2}x",
        steady_ns / early_ns.max(1.0)
    );
    // Pre-fix, acc_commitment rehashed the whole `seen` set every call, so
    // per-call cost scaled with k (measured 61x between k=0->1 and k=264->265
    // in foculus/audit/fold-step-cost.md). The incremental digest makes
    // acc_commitment O(1) in k; a generous 5x bound still catches a
    // regression back to O(k) (which would show >10x over an 11x growth in k)
    // while tolerating scheduler noise between the two batches.
    assert!(
        steady_ns < early_ns * 5.0,
        "absorb_ticket cost grew {:.2}x from k=50 to k=550 — acc_commitment may be rehashing `seen` again",
        steady_ns / early_ns.max(1.0)
    );
}

fn built(n: usize, from: u64, count: u64) -> ClusterAcc {
    let mut acc = ClusterAcc::empty(n);
    for i in from..from + count {
        absorb_ticket(&mut acc, &ticket(i, n));
    }
    acc
}

#[test]
fn fold_acc_disjoint_cost_does_not_scale_with_k() {
    // fold_acc's BTreeSet union of `seen` is still O(k) — unavoidable without
    // a persistent set structure, and out of scope here. This checks only
    // that the O(k) *rehash inside acc_commitment* is gone: folding two 8x
    // larger accumulators should cost far less than 8x more, not ~8x more.
    let n = 8;
    let small_a = built(n, 0, 16);
    let small_b = built(n, 16, 16);
    let t = Instant::now();
    let small_out = fold_acc(&small_a, &small_b);
    let small_ns = t.elapsed().as_nanos().max(1) as f64;

    let big_a = built(n, 0, 128);
    let big_b = built(n, 128, 128);
    let t = Instant::now();
    let big_out = fold_acc(&big_a, &big_b);
    let big_ns = t.elapsed().as_nanos().max(1) as f64;

    println!(
        "[incremental-acc] fold_acc: k=16+16 {small_ns:.0}ns, k=128+128 {big_ns:.0}ns, ratio={:.2}x for 8x the k",
        big_ns / small_ns
    );
    assert_eq!(small_out.k, 32);
    assert_eq!(big_out.k, 256);
    // Pre-fix, acc_commitment rehashed all of `seen` on every fold, so cost
    // tracked k directly (~8x here). Post-fix the BTreeSet union still costs
    // more work at 8x the elements, but nowhere near the ~8x a full rehash
    // would add on top of it; a 6x bound leaves headroom for noise while
    // still catching a regression back to O(k) commitment hashing.
    assert!(
        big_ns < small_ns * 6.0,
        "fold_acc cost scaled {:.2}x for 8x the k — acc_commitment may be rehashing `seen` again",
        big_ns / small_ns
    );
}

#[test]
fn fold_acc_overlap_digest_is_order_independent() {
    // a and b share tickets 10..20 — exercises fold_acc's inclusion-exclusion
    // correction (seen_digest = left + right - intersection) rather than the
    // disjoint fast path.
    let n = 4;
    let mut a = ClusterAcc::empty(n);
    for i in 0..20 {
        absorb_ticket(&mut a, &ticket(i, n));
    }
    let mut b = ClusterAcc::empty(n);
    for i in 10..30 {
        absorb_ticket(&mut b, &ticket(i, n));
    }
    let ab = fold_acc(&a, &b);
    let ba = fold_acc(&b, &a);
    assert_eq!(ab.seen.len(), 30);
    assert_eq!(ab.seen, ba.seen);
    assert_eq!(ab.commitment, ba.commitment);
}
