use super::*;

fn g(v: u64) -> F {
    F::new(v)
}

struct Fixture {
    t: SparseGraph,
    s: SparseGraph,
    degree: Vec<F>,
    teleport: Vec<F>,
    phi0: Vec<F>,
    params: TriKernelParams,
}

impl Fixture {
    fn new() -> Self {
        let n = 4;
        let mut t = SparseGraph::empty(n);
        let mut s = SparseGraph::empty(n);
        for i in 0..n {
            t.add((i + 1) % n, i, g(1));
            s.add(i, (i + 1) % n, g(1));
            s.add((i + 1) % n, i, g(1));
        }
        t.add(2, 0, g(3));
        let inv4 = g(4).inv();
        Self {
            t,
            s,
            degree: vec![g(2); n],
            teleport: vec![inv4; n],
            phi0: vec![g(1), g(0), g(0), g(0)],
            params: TriKernelParams::standard(),
        }
    }
    fn graph(&self) -> PhiGraph<'_> {
        PhiGraph {
            transition: &self.t,
            sym: &self.s,
            degree: &self.degree,
            teleport: &self.teleport,
            params: &self.params,
        }
    }
}

/// φ* after 6 steps and the graph commitments, as zheng 0.4's
/// `phi::prove_phi_star` and `SparseGraph::commitment` produced them for this
/// fixture (feature `legacy`, before the port left zheng behind).
#[test]
fn phi_star_matches_the_zheng_vectors() {
    let f = Fixture::new();
    let ours = compute_phi_star(&f.graph(), &f.phi0, 6).unwrap();
    assert_eq!(
        ours.phi_star.iter().map(|v| v.as_u64()).collect::<Vec<_>>(),
        [13624355315955472899, 2999415092397749681, 6972205826464312662, 13297511904011633401]
    );
    assert_eq!(
        f.t.commitment(),
        [
            243, 42, 178, 220, 193, 59, 124, 67, 120, 191, 136, 50, 236, 135, 202, 201, 166, 40,
            63, 111, 23, 176, 90, 200, 128, 98, 155, 254, 239, 117, 55, 31
        ]
    );
    assert_eq!(
        f.s.commitment(),
        [
            66, 245, 177, 166, 177, 33, 12, 233, 103, 42, 16, 63, 248, 175, 56, 197, 25, 69, 33,
            158, 132, 163, 122, 204, 16, 137, 215, 33, 12, 143, 223, 61
        ]
    );
    assert_eq!(
        f.graph().commitment(),
        [
            30, 53, 123, 32, 139, 107, 72, 242, 18, 215, 49, 68, 230, 242, 197, 200, 245, 227,
            243, 159, 108, 11, 22, 163, 114, 124, 127, 209, 19, 62, 106, 140
        ]
    );
}

#[test]
fn honest_claim_verifies() {
    let f = Fixture::new();
    let claim = compute_phi_star(&f.graph(), &f.phi0, 5).unwrap();
    assert!(verify_phi_claim(&claim.binding, &f.graph(), &f.phi0));
}

#[test]
fn claim_is_bound_to_its_graph_phi0_and_result() {
    let f = Fixture::new();
    let claim = compute_phi_star(&f.graph(), &f.phi0, 5).unwrap();
    let mut other = Fixture::new();
    other.t.add(3, 1, g(2));
    assert!(!verify_phi_claim(&claim.binding, &other.graph(), &f.phi0));
    let mut teleport = Fixture::new();
    teleport.teleport[0] = g(1);
    assert!(!verify_phi_claim(&claim.binding, &teleport.graph(), &f.phi0));
    assert!(!verify_phi_claim(&claim.binding, &f.graph(), &[g(0), g(1), g(0), g(0)]));
    let mut forged = claim.binding;
    forged.phi_star_hash[0] ^= 1;
    assert!(!verify_phi_claim(&forged, &f.graph(), &f.phi0));
    let mut fewer = claim.binding;
    fewer.iterations -= 1;
    assert!(!verify_phi_claim(&fewer, &f.graph(), &f.phi0));
}

#[test]
fn malformed_graphs_fail_without_panicking() {
    let mut f = Fixture::new();
    let claim = compute_phi_star(&f.graph(), &f.phi0, 2).unwrap();
    f.s.edges.push((9, 0, g(1)));
    assert!(!verify_phi_claim(&claim.binding, &f.graph(), &f.phi0));
    let f = Fixture::new();
    assert!(!verify_phi_claim(&claim.binding, &f.graph(), &f.phi0[..3]));
}
