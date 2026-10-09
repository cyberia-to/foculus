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

/// The port and zheng's own tri-kernel produce the same φ*.
#[test]
fn phi_star_matches_zheng() {
    let f = Fixture::new();
    let ours = compute_phi_star(&f.graph(), &f.phi0, 6).unwrap();
    let theirs =
        zheng::phi::prove_phi_star(&f.t, &f.s, &f.degree, &f.teleport, &f.phi0, 6, &f.params)
            .unwrap();
    assert_eq!(ours.phi_star, theirs.phi_star);
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
