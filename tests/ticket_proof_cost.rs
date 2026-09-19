//! Property #3 (cyber/launch.md): ticket proof cost < ticket reward.
//!
//! specs/fold-mining.md claims the root decider is O(1): ~825 constraints,
//! ~10-50us verify, independent of how many settlement tickets were folded
//! into the accumulator. This measures decide (`seal`) and verify
//! (`verify_fold_seal`) wall time across growing ticket counts on real
//! `grind_settlement` output, and asserts the O(1) shape: verify time at the
//! largest batch must not scale with ticket count. Absolute numbers are
//! recorded in `audit/ticket-proof-cost.md`, not asserted here — wall-clock
//! is machine-dependent, only the flat shape is a property of the code.

use std::time::Instant;

use foculus::settlement::Contribution;
use foculus::ticket_proof::{TicketProver, verify_fold_seal};
use foculus::tickets::{easy_target, grind_settlement};
use tru::{Context, FocusingParams, Fx, Link};

fn h(b: u8) -> [u8; 32] {
    let mut x = [0u8; 32];
    x[0] = b;
    x
}

fn setup() -> (Vec<Link>, Vec<Contribution>, [u8; 32], [u8; 32]) {
    let base = vec![
        Link::stake(h(1), h(2), 100),
        Link::stake(h(2), h(3), 100),
        Link::stake(h(3), h(1), 100),
    ];
    let contribs = vec![Contribution {
        neuron: h(10),
        links: vec![Link::stake(h(2), h(1), 8000)],
        surprise: Fx::ONE,
    }];
    (base, contribs, h(0xBE), h(0xC1))
}

/// Fold + decide + verify wall time for `n` tickets, isolated. Returns
/// (fold_only, decide_only, verify_only).
fn measure(n: usize) -> (std::time::Duration, std::time::Duration, std::time::Duration) {
    let (base, contribs, beacon, cluster) = setup();
    let tickets = grind_settlement(
        &base,
        &contribs,
        &Context::none(),
        &FocusingParams::default(),
        &beacon,
        &cluster,
        &h(0x91),
        0,
        n as u64,
        n,
        easy_target(),
    );
    assert_eq!(tickets.len(), n, "grind must produce every requested ticket under an always-win target");

    let mut prover = TicketProver::new();
    let fold_start = Instant::now();
    for t in &tickets {
        prover.fold_settlement(&beacon, &cluster, t).expect("fold_settlement");
    }
    let fold_only = fold_start.elapsed();

    let decide_start = Instant::now();
    let seal = prover.seal(&beacon, &cluster, tickets.len() as u64).expect("seal");
    let decide_only = decide_start.elapsed();

    let verify_start = Instant::now();
    assert!(verify_fold_seal(&seal));
    let verify_only = verify_start.elapsed();

    (fold_only, decide_only, verify_only)
}

#[test]
fn decide_and_verify_are_flat_in_ticket_count() {
    let sizes = [1usize, 8, 64, 512];
    let mut verify_times = Vec::with_capacity(sizes.len());

    for &n in &sizes {
        let (fold_only, decide_only, verify_only) = measure(n);
        eprintln!(
            "n={n:>4}  fold_only={fold_only:>10?}  decide_only={decide_only:>10?}  verify_only={verify_only:>10?}"
        );
        verify_times.push(verify_only);
    }

    // O(1) shape: verify at the largest batch must stay within the same
    // order of magnitude as verify at the smallest, not scale linearly with
    // ticket count (512x). A generous 20x guard absorbs measurement noise
    // on a shared CI machine while still catching an O(n) regression.
    let smallest = verify_times.first().copied().unwrap();
    let largest = verify_times.last().copied().unwrap();
    assert!(
        largest <= smallest * 20 + std::time::Duration::from_millis(5),
        "verify time grew with ticket count: {smallest:?} at n={} vs {largest:?} at n={}; \
         decide/verify must be O(1) per specs/fold-mining.md",
        sizes[0],
        sizes[sizes.len() - 1]
    );
}
