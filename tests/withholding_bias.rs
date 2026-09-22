//! Property #7 — withholding bias bounded by compute share
//! (tru/specs/rewards.md §7 "Residual: withholding", specs/fold-mining.md
//! "withholding bias").
//!
//! A miner that is also a cluster contender can compute a candidate ticket's
//! marginal before publishing it, and decline to publish (withhold) any
//! ticket that would lower its own share — it cannot lie about a published
//! ticket, only abstain. This test grinds a real pool of winning tickets,
//! has a simulated adversary controlling a fraction `q` of that pool
//! withhold every sample below the pool's true mean, and checks the
//! resulting bias against the analytic worst-case bound derived in
//! specs/fold-mining.md: `bias(q) <= (mean - min) * q / (1 - q)`.

use foculus::settlement::Contribution;
use foculus::tickets::{grind_settlement, self_fold, SettlementTicket};
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

/// 6 contributors; contributor 0 is the adversary.
fn cluster() -> Vec<Contribution> {
    (0..6)
        .map(|i| Contribution {
            neuron: h((16 + i) as u8),
            links: vec![Link::stake(h(2), h(1), 100 + i as u128 * 900)],
            surprise: Fx::ONE,
        })
        .collect()
}

/// Deterministic xorshift64, seeded per call site — no dependency on ticket
/// order or content, so assignment is independent of the marginal being
/// assigned (an adversary with real compute share q wins ~q of the pool
/// regardless of sample value).
fn assign_to_adversary(nonce: u64, q: f64) -> bool {
    let mut x = nonce ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    (x % 1_000_000) as f64 / 1_000_000.0 < q
}

fn mean_and_min(tickets: &[SettlementTicket], idx: usize) -> (Fx, Fx) {
    let sum = tickets
        .iter()
        .fold(Fx::ZERO, |acc, t| acc + t.marginals[idx]);
    let mean = sum.div(Fx::from_int(tickets.len() as i64));
    let min = tickets
        .iter()
        .map(|t| t.marginals[idx])
        .reduce(|a, b| if b < a { b } else { a })
        .expect("nonempty pool");
    (mean, min)
}

/// Simulated bias at adversary compute share `q`: adversary withholds its
/// assigned tickets whose own marginal (index 0) is below the true pool
/// mean; honest tickets always publish.
fn biased_share(tickets: &[SettlementTicket], true_mean: Fx, q: f64) -> Fx {
    let published: Vec<SettlementTicket> = tickets
        .iter()
        .filter(|t| !assign_to_adversary(t.nonce, q) || t.marginals[0] >= true_mean)
        .cloned()
        .collect();
    let acc = self_fold(6, &published);
    acc.mean_shares(&cluster().iter().map(|c| c.neuron).collect::<Vec<_>>())[0].1
}

#[test]
fn withholding_bias_is_bounded_by_compute_share() {
    let c = cluster();
    let beacon = h(0xBE);
    let cluster_id = h(0xC1);
    let tickets = grind_settlement(
        &base(),
        &c,
        &Context::none(),
        &FocusingParams::default(),
        &beacon,
        &cluster_id,
        &h(0x91),
        0,
        4_000,
        400,
        u64::MAX >> 3, // real, non-trivial target — not every nonce wins
    );
    assert!(tickets.len() >= 200, "need a real pool, got {}", tickets.len());

    let (true_mean, min) = mean_and_min(&tickets, 0);
    let range = true_mean - min;
    assert!(range > Fx::ZERO, "degenerate pool, no spread to withhold against");

    let mut prev_bias = Fx::ZERO;
    for &q in &[0.05, 0.1, 0.25, 0.5] {
        let biased = biased_share(&tickets, true_mean, q);
        let bias = biased - true_mean;
        let bound = range * Fx::from_ratio((q * 1000.0) as i64, (1000.0 * (1.0 - q)) as i64);

        eprintln!(
            "[withholding-bias] q={q:.2} bias={:.6} bound={:.6}",
            bias.to_f64(),
            bound.to_f64()
        );

        assert!(bias >= Fx::ZERO, "withholding below-mean samples must not lower the adversary's own apparent share (q={q})");
        assert!(
            bias <= bound,
            "bias {:.6} exceeds the analytic bound {:.6} at q={q} — injectable bias is not bounded by compute share",
            bias.to_f64(),
            bound.to_f64()
        );
        assert!(
            bias >= prev_bias,
            "bias should be non-decreasing in compute share q (q={q})"
        );
        prev_bias = bias;
    }
}
