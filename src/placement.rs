// ---
// tags: sync, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Capacity-weighted shard placement, split out of `node` (which is gated
//! behind the `net` feature) so it can be unit-tested without pulling in
//! iroh/tokio.

/// Assign each of `n_shards` erasure shards to a device index: `0` is self,
/// `1..=peer_capacities.len()` are peers in the order given, weighted by
/// declared byte capacity.
///
/// A declared capacity of `0` means "unlimited" (see `node::format_bytes`,
/// which prints it that way), including self's own capacity — self never
/// declares a byte figure at all. An unlimited device resolves to the
/// largest known *finite* capacity among the other devices, so it is
/// weighted generously without mathematically swallowing every shard: a
/// literal `u64::MAX` for self, or the old `.max(1)` treatment of a `0`
/// peer, both broke the "weighted" part of capacity-weighted placement —
/// the former left peers with real capacity getting a ~0 share, the latter
/// crushed an operator's own "unlimited" declaration to the smallest
/// possible share.
pub fn capacity_weighted_placement(n_shards: usize, peer_capacities: &[u64]) -> Vec<usize> {
    let n_devices = peer_capacities.len() + 1;
    if n_shards == 0 {
        return Vec::new();
    }
    let finite_max = peer_capacities
        .iter()
        .copied()
        .filter(|&c| c != 0)
        .max()
        .unwrap_or(1);
    let resolve = |c: u64| if c == 0 { finite_max } else { c };
    let mut caps: Vec<u64> = Vec::with_capacity(n_devices);
    caps.push(finite_max); // self: unlimited, resolved the same way as any other unlimited device
    caps.extend(peer_capacities.iter().copied().map(resolve));

    let total_cap: u128 = caps.iter().map(|&c| c as u128).sum();
    let mut alloc = vec![0usize; n_devices];
    let mut assigned = 0;
    for d in 0..n_devices {
        let share = (n_shards as u128 * caps[d] as u128 / total_cap) as usize;
        alloc[d] = share;
        assigned += share;
    }
    let mut remainder = n_shards.saturating_sub(assigned);
    let mut order: Vec<usize> = (0..n_devices).collect();
    order.sort_by(|&a, &b| caps[b].cmp(&caps[a]));
    for &d in &order {
        if remainder == 0 {
            break;
        }
        alloc[d] += 1;
        remainder -= 1;
    }

    let mut placement = Vec::with_capacity(n_shards);
    for (device_idx, &count) in alloc.iter().enumerate() {
        for _ in 0..count {
            placement.push(device_idx);
        }
    }
    placement.truncate(n_shards);
    placement
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn counts(placement: &[usize]) -> HashMap<usize, usize> {
        let mut m = HashMap::new();
        for &d in placement {
            *m.entry(d).or_insert(0) += 1;
        }
        m
    }

    #[test]
    fn no_peers_keeps_everything_local() {
        let p = capacity_weighted_placement(8, &[]);
        assert_eq!(p.len(), 8);
        assert!(p.iter().all(|&d| d == 0));
    }

    #[test]
    fn zero_shards_is_empty() {
        assert!(capacity_weighted_placement(0, &[1_000, 2_000]).is_empty());
    }

    #[test]
    fn equal_real_capacities_split_evenly() {
        // self + 3 peers, all declaring the same real capacity: an even
        // four-way split, not self keeping everything.
        let p = capacity_weighted_placement(8, &[1_000, 1_000, 1_000]);
        let c = counts(&p);
        assert_eq!(c.len(), 4, "every device should get a share: {c:?}");
        for d in 0..4 {
            assert_eq!(c[&d], 2);
        }
    }

    #[test]
    fn self_does_not_swallow_every_shard_when_peers_have_real_capacity() {
        // Regression: self used to be weighted u64::MAX, so with any real
        // peer capacity present it took ~100% of the shards regardless of
        // how many peers were available to hold the rest.
        let p = capacity_weighted_placement(100, &[1_000, 1_000, 1_000, 1_000]);
        let c = counts(&p);
        let self_share = *c.get(&0).unwrap_or(&0);
        assert!(
            self_share < 100,
            "self kept every shard despite four equally-capable peers: {c:?}"
        );
        assert_eq!(c.len(), 5, "all five devices should receive at least one shard: {c:?}");
    }

    #[test]
    fn declared_unlimited_peer_is_not_starved() {
        // Regression: a peer that declares capacity 0 ("unlimited", per
        // node::format_bytes) used to be weighted `.max(1)` — one byte,
        // the smallest possible share — the opposite of what "unlimited"
        // should mean.
        let p = capacity_weighted_placement(100, &[1_000, 0]);
        let c = counts(&p);
        let unlimited_peer_share = *c.get(&2).unwrap_or(&0);
        assert!(
            unlimited_peer_share > 1,
            "a peer declaring unlimited capacity was starved to a token share: {c:?}"
        );
    }

    #[test]
    fn all_unlimited_splits_evenly() {
        let p = capacity_weighted_placement(9, &[0, 0]);
        let c = counts(&p);
        assert_eq!(c.len(), 3);
        for d in 0..3 {
            assert_eq!(c[&d], 3);
        }
    }

    #[test]
    fn placement_length_matches_shard_count() {
        let p = capacity_weighted_placement(37, &[500, 1_500, 0, 42]);
        assert_eq!(p.len(), 37);
        assert!(p.iter().all(|&d| d < 5));
    }

    #[test]
    fn a_larger_real_capacity_gets_a_proportionally_larger_share() {
        // Two real peers at a 3:1 capacity ratio should end up at
        // roughly a 3:1 share ratio between themselves.
        let p = capacity_weighted_placement(700, &[3_000, 1_000]);
        let c = counts(&p);
        assert_eq!(c[&1], 300, "{c:?}");
        assert_eq!(c[&2], 100, "{c:?}");
    }
}
