// ---
// tags: foculus, rust, rewards, cluster
// crystal-type: source
// crystal-domain: cyber
// ---
//! The canonical partition of an epoch's claims into clusters ([[reward
//! specification]] §7): "a cluster is a connected component of overlapping
//! ε-supports". Two pieces, both pure functions of the graph:
//!
//! - [`epsilon_support`] — the region a claim touches: a bounded-radius
//!   neighborhood of the particles its links name, in the claim's link graph.
//!   The spec derives the radius from ε and the local mixing rate
//!   (`r ≈ log(1/ε)/log(1/λ_local)`); that derivation needs a per-region
//!   spectral estimate this crate does not yet compute, so the radius is
//!   taken as an explicit protocol parameter here. Deriving it from ε is the
//!   next slice.
//! - [`partition_into_clusters`] — claims whose supports overlap merge into
//!   one cluster, by union-find; clusters that share no particle stay
//!   independent, so settlement over them parallelizes.
//!
//! Both are deterministic functions of the edges, the seed particles and the
//! radius, so the partition is the same for every honest neuron regardless
//! of claim arrival order — no miner draws a self-serving boundary.

use std::collections::{BTreeMap, BTreeSet};

use cyber_hemera::hash as hemera_hash;

use crate::rewards::RewardClaim;
use crate::tickets::ClusterId;

/// Domain separation for the cluster id hash.
const DOMAIN: &[u8] = b"foculus-cluster-v0";

/// Adjacency of the epoch's link graph: particle → particles it links to or
/// from, deduplicated. Callers build this from the same claims being
/// partitioned (or a wider context), since overlap is a property of the
/// whole graph, not of one claim alone.
pub type Adjacency = BTreeMap<[u8; 32], Vec<[u8; 32]>>;

/// The particles one claim names directly: every `from` and `to` of its
/// links. This is the seed of its ε-support, radius zero.
pub fn claim_particles(claim: &RewardClaim) -> BTreeSet<[u8; 32]> {
    let mut particles = BTreeSet::new();
    for link in &claim.links {
        particles.insert(link.from);
        particles.insert(link.to);
    }
    particles
}

/// Bounded-radius neighborhood of `seed` in `adjacency`: every particle
/// reachable within `radius` hops. `radius = 0` returns `seed` itself — the
/// smallest possible support, content-dependent through `adjacency` alone.
pub fn epsilon_support(
    seed: &BTreeSet<[u8; 32]>,
    adjacency: &Adjacency,
    radius: usize,
) -> BTreeSet<[u8; 32]> {
    let mut support: BTreeSet<[u8; 32]> = seed.clone();
    let mut frontier: Vec<[u8; 32]> = seed.iter().copied().collect();
    for _ in 0..radius {
        let mut next = Vec::new();
        for particle in &frontier {
            if let Some(neighbors) = adjacency.get(particle) {
                for n in neighbors {
                    if support.insert(*n) {
                        next.push(*n);
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    support
}

/// Union-find over claim indices, merged when their ε-supports overlap.
/// Returns clusters as groups of claim indices, members ordered by claim
/// id and the groups by their smallest claim id — so the listing, not just
/// the partition, is independent of the order claims were passed in.
pub fn partition_into_clusters(
    claims: &[RewardClaim],
    adjacency: &Adjacency,
    radius: usize,
) -> Vec<Vec<usize>> {
    let supports: Vec<BTreeSet<[u8; 32]>> = claims
        .iter()
        .map(|c| epsilon_support(&claim_particles(c), adjacency, radius))
        .collect();

    let mut parent: Vec<usize> = (0..claims.len()).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }

    for i in 0..supports.len() {
        for j in (i + 1)..supports.len() {
            if !supports[i].is_disjoint(&supports[j]) {
                union(&mut parent, i, j);
            }
        }
    }

    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..claims.len() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }

    let mut clusters: Vec<Vec<usize>> = groups.into_values().collect();
    for members in &mut clusters {
        members.sort_by_key(|&i| claims[i].id);
    }
    clusters.sort_by_key(|members| members.first().map(|&i| claims[i].id));
    clusters
}

/// The `ClusterId` a partition's members bind to: a domain-separated hash of
/// their sorted claim ids, matching `tickets::ClusterId`'s "explicit cluster
/// particle" binding. Deterministic under any input order.
pub fn cluster_id(claims: &[RewardClaim], members: &[usize]) -> ClusterId {
    let mut ids: Vec<[u8; 32]> = members.iter().map(|&i| claims[i].id).collect();
    ids.sort_unstable();
    let mut buf = Vec::with_capacity(DOMAIN.len() + 8 + ids.len() * 32);
    buf.extend_from_slice(DOMAIN);
    buf.extend_from_slice(&(ids.len() as u64).to_le_bytes());
    for id in &ids {
        buf.extend_from_slice(id);
    }
    *hemera_hash(&buf)
        .as_bytes()
        .first_chunk::<32>()
        .unwrap_or(&[0u8; 32])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tru::{Fx, Link};

    fn claim(id: u8, from: u8, to: u8) -> RewardClaim {
        RewardClaim {
            id: [id; 32],
            neuron: [id; 32],
            links: vec![Link::stake([from; 32], [to; 32], 1)],
            belief: Fx::ONE,
            prediction: Fx::ONE,
        }
    }

    #[test]
    fn disjoint_claims_stay_in_separate_clusters() {
        let claims = vec![claim(1, 10, 11), claim(2, 20, 21)];
        let adjacency = Adjacency::new();
        let clusters = partition_into_clusters(&claims, &adjacency, 0);
        assert_eq!(clusters, vec![vec![0], vec![1]]);
    }

    #[test]
    fn overlapping_supports_merge_into_one_cluster() {
        // claim 0 touches {10,11}, claim 1 touches {20,21}; an edge 11→20
        // brings them into the same radius-1 support, so they merge.
        let claims = vec![claim(1, 10, 11), claim(2, 20, 21)];
        let mut adjacency = Adjacency::new();
        adjacency.insert([11; 32], vec![[20; 32]]);
        let clusters = partition_into_clusters(&claims, &adjacency, 1);
        assert_eq!(clusters, vec![vec![0, 1]]);
    }

    #[test]
    fn partition_is_canonical_across_input_order() {
        // Claims 1 and 3 meet through the graph (11→30 at radius 1); claim 2
        // is off on its own. Whatever order the claims arrive in, the
        // partition must be {1,3} | {2}, listed in the same canonical order.
        let forward = vec![claim(1, 10, 11), claim(2, 20, 21), claim(3, 30, 31)];
        let reversed = vec![claim(3, 30, 31), claim(2, 20, 21), claim(1, 10, 11)];
        let mut adjacency = Adjacency::new();
        adjacency.insert([11; 32], vec![[30; 32]]);

        let ids_of = |claims: &[RewardClaim], clusters: &[Vec<usize>]| -> Vec<Vec<[u8; 32]>> {
            clusters
                .iter()
                .map(|members| members.iter().map(|&i| claims[i].id).collect())
                .collect()
        };
        let a = ids_of(&forward, &partition_into_clusters(&forward, &adjacency, 1));
        let b = ids_of(&reversed, &partition_into_clusters(&reversed, &adjacency, 1));

        assert_eq!(a, vec![vec![[1; 32], [3; 32]], vec![[2; 32]]]);
        assert_eq!(a, b, "the partition and its listing must not depend on claim arrival order");
        assert_eq!(
            cluster_id(&forward, &[0, 2]),
            cluster_id(&reversed, &[2, 0]),
            "the cluster id binds the same members under either arrival order"
        );
    }

    #[test]
    fn cluster_id_is_stable_under_member_order() {
        let claims = vec![claim(1, 10, 11), claim(2, 20, 21)];
        let id_forward = cluster_id(&claims, &[0, 1]);
        let id_reversed = cluster_id(&claims, &[1, 0]);
        assert_eq!(id_forward, id_reversed);
    }

    #[test]
    fn radius_zero_support_is_just_the_seed() {
        let claims = vec![claim(1, 10, 11)];
        let mut adjacency = Adjacency::new();
        adjacency.insert([11; 32], vec![[99; 32]]);
        let support = epsilon_support(&claim_particles(&claims[0]), &adjacency, 0);
        assert_eq!(support, claim_particles(&claims[0]));
    }
}
