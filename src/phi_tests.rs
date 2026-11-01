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
/// `phi::prove_phi_star` and `SparseGraph::commitment` produce them for this
/// fixture (feature `legacy`) on cyber-hemera 0.4: zheng 166a14e, hemera
/// 8559b52, soft3 diagnostic run 41332954376, where zheng's and foculus's φ*,
/// transition and symmetric commitments are equal. The `PhiGraph` commitment
/// has no zheng counterpart (domain `foculus-phi-graph-v1`); it is foculus's
/// digest over those two commitments, the degree, the teleport and the params,
/// from the same run. The first freeze (87603ec) was taken on hemera 0.3.1.
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
            75, 238, 46, 9, 134, 44, 34, 92, 4, 185, 157, 75, 130, 82, 26, 185, 254, 244, 204, 216,
            198, 148, 245, 246, 210, 58, 40, 24, 63, 226, 78, 191
        ]
    );
    assert_eq!(
        f.s.commitment(),
        [
            46, 164, 29, 123, 244, 18, 210, 238, 66, 62, 209, 140, 122, 241, 68, 166, 109, 81, 34,
            76, 79, 3, 52, 138, 112, 185, 20, 244, 80, 151, 86, 152
        ]
    );
    assert_eq!(
        f.graph().commitment(),
        [
            240, 136, 171, 145, 148, 200, 92, 156, 159, 178, 123, 103, 97, 9, 212, 208, 134, 167,
            82, 208, 172, 205, 200, 52, 129, 245, 108, 5, 13, 175, 133, 20
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
