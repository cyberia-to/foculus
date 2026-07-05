//! settlement — the leaderless Shapley lottery (`specs/fold-mining.md`).
//!
//! The first lottery of the reward pipeline. It estimates the fair division of a
//! cluster's Δφ⁺ by beacon-seeded sampling — the settlement mining of the
//! [reward specification] §7. It sits on a clean architectural seam:
//!
//! - **magnitude & marginal** are [`tru`]'s, a pure function of the graph:
//!   `tru::attribution::value` (v★) and `tru::attribution::marginals` (the
//!   per-ordering marginal). foculus never recomputes focus.
//! - **the lottery** is foculus's, because it needs the epoch beacon and
//!   leaderless consensus tru does not have: which orderings are drawn (seeded
//!   by the beacon), how they are sampled, and how the samples aggregate.
//!
//! foculus draws `π(n)` from the beacon, asks tru for the marginal `m(n)` under
//! that ordering, and averages the swarm's samples — converging to the exact
//! Shapley value tru defines by full enumeration ([`tru::attribution::shapley_exact`]).
//! Conservation and the mint are `tok`.

use tru::attribution::{marginals, Contribution};
use tru::{Context, Fx, FocusingParams, Link};

/// A deterministic permutation of `0..n` seeded by `beacon ‖ nonce` — the
/// settlement ordering `π(n)` (§7). Fisher–Yates driven by a hemera stream.
/// The beacon lives here, in foculus: it is the epoch randomness tru cannot see.
pub fn ordering(n: usize, beacon: &[u8; 32], nonce: u64) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..n).collect();
    if n < 2 {
        return perm;
    }
    let mut buf = [0u8; 40];
    buf[..32].copy_from_slice(beacon);
    buf[32..].copy_from_slice(&nonce.to_le_bytes());
    let mut digest = *cyber_hemera::hash(&buf).as_bytes();
    let mut byte = 0usize;
    let next = |digest: &mut [u8; 32], byte: &mut usize| -> u64 {
        if *byte + 8 > 32 {
            *digest = *cyber_hemera::hash(digest).as_bytes();
            *byte = 0;
        }
        let v = u64::from_le_bytes(digest[*byte..*byte + 8].try_into().unwrap());
        *byte += 8;
        v
    };
    for i in (1..n).rev() {
        let j = (next(&mut digest, &mut byte) % (i as u64 + 1)) as usize;
        perm.swap(i, j);
    }
    perm
}

/// `Shapley(v★)` by Monte-Carlo over `samples` beacon-seeded orderings (§4, §7):
/// the leaderless estimator of the fair division. Each ordering is drawn from
/// the beacon; each sample is a [`marginals`] call into tru. Returns
/// `(neuron, share)` in contribution order.
///
/// This is the swarm's job in aggregate — one miner runs a slice of the
/// `samples` and publishes winning tickets; the sum converges to
/// [`tru::attribution::shapley_exact`] by Hoeffding. Conservation (clipping to
/// realized Δφ⁺) is `tok`'s step.
pub fn shapley(
    base: &[Link],
    contribs: &[Contribution],
    ctx: &Context,
    params: &FocusingParams,
    samples: u64,
    beacon: &[u8; 32],
) -> Vec<([u8; 32], Fx)> {
    let n = contribs.len();
    if n == 0 || samples == 0 {
        return contribs.iter().map(|c| (c.neuron, Fx::ZERO)).collect();
    }
    let mut acc = vec![Fx::ZERO; n];
    for s in 0..samples {
        let order = ordering(n, beacon, s);
        let m = marginals(base, contribs, &order, ctx, params);
        for i in 0..n {
            acc[i] = acc[i] + m[i];
        }
    }
    let inv = Fx::ONE.div(Fx::from_int(samples as i64));
    contribs
        .iter()
        .enumerate()
        .map(|(i, c)| (c.neuron, acc[i] * inv))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tru::attribution::{shapley_exact, value};

    fn hash(b: u8) -> [u8; 32] {
        let mut h = [0u8; 32];
        h[0] = b;
        h
    }

    fn contrib(neuron: u8, links: Vec<Link>, rho: Fx) -> Contribution {
        Contribution { neuron: hash(neuron), links, surprise: rho }
    }

    fn beacon() -> [u8; 32] {
        hash(0xBE)
    }

    fn base() -> Vec<Link> {
        vec![
            Link::stake(hash(1), hash(2), 100),
            Link::stake(hash(2), hash(3), 100),
            Link::stake(hash(3), hash(1), 100),
        ]
    }

    #[test]
    fn ordering_is_deterministic_and_a_permutation() {
        let p1 = ordering(6, &beacon(), 3);
        assert_eq!(p1, ordering(6, &beacon(), 3), "same beacon+nonce → same ordering");
        let mut sorted = p1.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4, 5], "a permutation of 0..n");
        assert_ne!(p1, ordering(6, &beacon(), 4), "different nonce → different ordering");
    }

    #[test]
    fn lottery_estimates_the_exact_shapley_division() {
        // The beacon-seeded Monte-Carlo estimate should track tru's exact
        // enumeration (which foculus never has to run at scale).
        let a = contrib(10, vec![Link::stake(hash(2), hash(1), 8000)], Fx::ONE);
        let b = contrib(11, vec![Link::stake(hash(3), hash(1), 6000)], Fx::ONE);
        let params = FocusingParams::default();
        let exact = shapley_exact(&base(), &[a, b], &Context::none(), &params);

        let a = contrib(10, vec![Link::stake(hash(2), hash(1), 8000)], Fx::ONE);
        let b = contrib(11, vec![Link::stake(hash(3), hash(1), 6000)], Fx::ONE);
        let est = shapley(&base(), &[a, b], &Context::none(), &params, 64, &beacon());

        // Monte-Carlo tolerance: the estimate tracks the exact division within
        // sampling error (deterministic here — a fixed beacon fixes the draws).
        for i in 0..2 {
            assert!(
                (est[i].1.to_f64() - exact[i].1.to_f64()).abs() < 5e-3,
                "estimate {} ≠ exact {} for contributor {i}",
                est[i].1.to_f64(),
                exact[i].1.to_f64()
            );
        }
    }

    #[test]
    fn lottery_is_efficient() {
        // Σ shares = v★(all), within Monte-Carlo error.
        let a = contrib(10, vec![Link::stake(hash(2), hash(1), 8000)], Fx::ONE);
        let b = contrib(11, vec![Link::stake(hash(3), hash(1), 6000)], Fx::ONE);
        let params = FocusingParams::default();
        let all = value(&base(), &[&a, &b], &Context::none(), &params);

        let a = contrib(10, vec![Link::stake(hash(2), hash(1), 8000)], Fx::ONE);
        let b = contrib(11, vec![Link::stake(hash(3), hash(1), 6000)], Fx::ONE);
        let est = shapley(&base(), &[a, b], &Context::none(), &params, 16, &beacon());
        let sum: f64 = est.iter().map(|s| s.1.to_f64()).sum();
        assert!((sum - all.to_f64()).abs() < 1e-3, "Σ shares {sum} ≠ v★(N) {}", all.to_f64());
    }
}
