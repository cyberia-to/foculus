//! settlement — the reward magnitude, division, and leaderless lottery
//! (`specs/fold-mining.md`, [reward specification] §4–§7).
//!
//! This is the reward layer. It owns the *cooperative game* over a cluster's
//! contributions and its *solution*: the surprise-weighted value `v★(S)`, the
//! per-contributor marginal, the exact Shapley division, and the beacon-seeded
//! lottery that estimates it leaderlessly. All of it is reward policy — it
//! changes when the reward mechanism changes, not when the intelligence layer
//! does.
//!
//! The one thing it does *not* compute is the physical value itself. The
//! magnitude of a focus shift, `Δφ⁺`, is a pure function of the graph and lives
//! in [`tru`]: `tru::impulse(base, batch).directed`. foculus builds the coalition
//! (applying surprise `ρ` to each contribution's links), asks tru for the
//! impulse, and does everything else. tru never sees the beacon or the game;
//! foculus never recomputes focus. Conservation and the mint are `tok`.

use tru::{impulse, Context, Fx, FocusingParams, Link};

/// One contributor's stake in a contested cluster: the neuron, its links, and
/// the per-contribution surprise `ρ` (from `tru::truth_scoring`). A reward-layer
/// type — it bundles a reward concept (surprise) with the raw links tru scores.
pub struct Contribution {
    pub neuron: [u8; 32],
    pub links: Vec<Link>,
    pub surprise: Fx,
}

impl Contribution {
    /// The links surprise-weighted for `v★`: each link's market multiplier folds
    /// in `ρ`, so `stake·κ·f(price)` becomes `ρ·stake·κ·f(price)` — a copy
    /// (`ρ→0`) contributes nothing, so the economy divides *surprising* syntropy.
    fn weighted(&self) -> Vec<Link> {
        let rho = clamp01(self.surprise);
        self.links
            .iter()
            .cloned()
            .map(|mut l| {
                l.price = clamp01(l.price) * rho;
                l
            })
            .collect()
    }
}

fn clamp01(x: Fx) -> Fx {
    if x < Fx::ZERO {
        Fx::ZERO
    } else if x > Fx::ONE {
        Fx::ONE
    } else {
        x
    }
}

/// `v★(S) = Δφ⁺(A^eff ∪ ρ·S)`: the surprise-weighted directed focus impulse of a
/// coalition. The characteristic function of the cooperative game — evaluated by
/// asking tru for the physical impulse of the ρ-weighted links.
pub fn value(base: &[Link], coalition: &[&Contribution], ctx: &Context, params: &FocusingParams) -> Fx {
    let mut batch = Vec::new();
    for c in coalition {
        batch.extend(c.weighted());
    }
    impulse(base, &batch, ctx, params, params.epsilon).directed
}

/// The per-contributor marginal along one `order`: `out[i] = v★(prefix ∪ {i}) −
/// v★(prefix)`, `prefix` being the contributors before `i`. The sample a
/// settlement miner computes for a given ordering (§7).
pub fn marginals(base: &[Link], contribs: &[Contribution], order: &[usize], ctx: &Context, params: &FocusingParams) -> Vec<Fx> {
    let mut out = vec![Fx::ZERO; contribs.len()];
    let mut prefix: Vec<&Contribution> = Vec::new();
    let mut prev = Fx::ZERO; // v★(∅) = 0
    for &i in order {
        prefix.push(&contribs[i]);
        let v = value(base, &prefix, ctx, params);
        out[i] = v - prev;
        prev = v;
    }
    out
}

/// The **exact** Shapley division of `v★` — the reference definition, averaging
/// [`marginals`] over all `n!` orderings. Deterministic and beacon-free, `O(n!)`:
/// the definition and small-cluster path. Production estimates it by the lottery
/// below. Conservation (clipping to realized Δφ⁺) is `tok`'s step.
pub fn shapley_exact(base: &[Link], contribs: &[Contribution], ctx: &Context, params: &FocusingParams) -> Vec<([u8; 32], Fx)> {
    let n = contribs.len();
    if n == 0 {
        return vec![];
    }
    let orders = permutations(n);
    let mut acc = vec![Fx::ZERO; n];
    for order in &orders {
        let m = marginals(base, contribs, order, ctx, params);
        for i in 0..n {
            acc[i] = acc[i] + m[i];
        }
    }
    let inv = Fx::ONE.div(Fx::from_int(orders.len() as i64));
    contribs.iter().enumerate().map(|(i, c)| (c.neuron, acc[i] * inv)).collect()
}

/// All permutations of `0..n` (Heap-style recursion). `O(n!)` — small `n` only.
fn permutations(n: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut cur: Vec<usize> = (0..n).collect();
    fn go(a: &mut Vec<usize>, k: usize, out: &mut Vec<Vec<usize>>) {
        if k == a.len() {
            out.push(a.clone());
            return;
        }
        for i in k..a.len() {
            a.swap(k, i);
            go(a, k + 1, out);
            a.swap(k, i);
        }
    }
    go(&mut cur, 0, &mut out);
    out
}

/// A deterministic permutation of `0..n` seeded by `beacon ‖ nonce` — the
/// settlement ordering `π(n)` (§7). Fisher–Yates driven by a hemera stream. The
/// beacon lives here in foculus: the epoch randomness tru cannot see.
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
/// the leaderless estimator. Each ordering is drawn from the beacon; each sample
/// is a [`marginals`] evaluation (which calls tru). The swarm's samples converge
/// to [`shapley_exact`] by Hoeffding. Returns `(neuron, share)` in contribution
/// order.
pub fn shapley(base: &[Link], contribs: &[Contribution], ctx: &Context, params: &FocusingParams, samples: u64, beacon: &[u8; 32]) -> Vec<([u8; 32], Fx)> {
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
    contribs.iter().enumerate().map(|(i, c)| (c.neuron, acc[i] * inv)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn value_of_empty_coalition_is_zero() {
        let v = value(&base(), &[], &Context::none(), &FocusingParams::default());
        assert_eq!(v.raw(), Fx::ZERO.raw());
    }

    #[test]
    fn a_copy_contributes_nothing() {
        let c = contrib(9, vec![Link::stake(hash(3), hash(1), 400)], Fx::ZERO); // ρ=0
        let v = value(&base(), &[&c], &Context::none(), &FocusingParams::default());
        assert_eq!(v.raw(), Fx::ZERO.raw(), "a ρ=0 copy must add zero value");
    }

    #[test]
    fn shapley_exact_is_symmetric_efficient_and_null() {
        let params = FocusingParams::default();
        // symmetry: same-edge contributors split equally.
        let a = contrib(10, vec![Link::stake(hash(2), hash(1), 8000)], Fx::ONE);
        let b = contrib(11, vec![Link::stake(hash(2), hash(1), 8000)], Fx::ONE);
        let s = shapley_exact(&base(), &[a, b], &Context::none(), &params);
        assert!((s[0].1.to_f64() - s[1].1.to_f64()).abs() < 1e-9, "symmetric split equally");

        // efficiency + null-copy.
        let a = contrib(10, vec![Link::stake(hash(2), hash(1), 8000)], Fx::ONE);
        let copy = contrib(11, vec![Link::stake(hash(3), hash(1), 8000)], Fx::ZERO); // ρ=0
        let all = value(&base(), &[&a, &copy], &Context::none(), &params);
        let a = contrib(10, vec![Link::stake(hash(2), hash(1), 8000)], Fx::ONE);
        let copy = contrib(11, vec![Link::stake(hash(3), hash(1), 8000)], Fx::ZERO);
        let s = shapley_exact(&base(), &[a, copy], &Context::none(), &params);
        let sum: f64 = s.iter().map(|x| x.1.to_f64()).sum();
        assert!((sum - all.to_f64()).abs() < 1e-9, "Σ Shapley = v★(N)");
        assert!(s[1].1.to_f64().abs() < 1e-9, "ρ=0 copy earns 0");
        assert!(s[0].1.to_f64() > 0.0, "the real contributor earns the value");
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
    fn lottery_estimates_the_exact_division() {
        let params = FocusingParams::default();
        let mk = || [contrib(10, vec![Link::stake(hash(2), hash(1), 8000)], Fx::ONE), contrib(11, vec![Link::stake(hash(3), hash(1), 6000)], Fx::ONE)];
        let exact = shapley_exact(&base(), &mk(), &Context::none(), &params);
        let est = shapley(&base(), &mk(), &Context::none(), &params, 64, &beacon());
        for i in 0..2 {
            assert!(
                (est[i].1.to_f64() - exact[i].1.to_f64()).abs() < 5e-3,
                "MC estimate {} ≠ exact {} for contributor {i}",
                est[i].1.to_f64(),
                exact[i].1.to_f64()
            );
        }
    }
}
