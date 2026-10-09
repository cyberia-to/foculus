// ---
// tags: foculus, rust, phi, tri-kernel
// crystal-type: source
// crystal-domain: cyber
// ---
//! φ* claims for epoch certificates, checked by direct recomputation.
//!
//! zheng's `verify_phi_star` re-runs the tri-kernel natively — that final
//! equality is what makes it sound — but it also verifies one legacy
//! HyperNova SpMV proof per iteration and never checks the claimed graph
//! commitment against the graphs it is handed (launch row 40). A public v3
//! certificate of the SpMV rows would disclose the intermediate vectors and
//! check the same products the recomputation does, at more bytes. So a
//! claim here carries no proof: the verifier recomputes `iterations` steps
//! over graphs whose commitment the certificate binds.
//!
//! Cost: `iterations × (4 SpMV + 3n field inversions)`. A succinct φ* proof
//! needs the tri-kernel as a step relation under accumulation (phase 3).
//!
//! [`trikernel_step`] is a port of zheng `phi::trikernel_step`, which zheng
//! does not export; `phi_star_matches_zheng` pins the two together.

use nebu::Goldilocks as F;
use zheng::phi::{SparseGraph, TriKernelParams};

use crate::proof::digest;

const GRAPH_DOMAIN: &[u8] = b"foculus-phi-graph-v1";
const VECTOR_DOMAIN: &[u8] = b"foculus-phi-vector-v1";

/// The public inputs of a φ* computation.
pub struct PhiGraph<'a> {
    pub transition: &'a SparseGraph,
    pub sym: &'a SparseGraph,
    pub degree: &'a [F],
    pub teleport: &'a [F],
    pub params: &'a TriKernelParams,
}

/// What an epoch certificate binds about φ*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhiBinding {
    pub graph_commit: [u8; 32],
    pub phi0_hash: [u8; 32],
    pub phi_star_hash: [u8; 32],
    pub iterations: u32,
}

/// A φ* claim: the binding plus the vector itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhiClaim {
    pub binding: PhiBinding,
    pub phi_star: Vec<F>,
}

fn field_bytes(v: &[F]) -> Vec<u8> {
    v.iter().flat_map(|x| x.as_u64().to_le_bytes()).collect()
}

pub fn vector_hash(v: &[F]) -> [u8; 32] {
    digest(VECTOR_DOMAIN, &[&field_bytes(v)])
}

impl PhiGraph<'_> {
    fn n(&self) -> usize {
        self.transition.n
    }

    /// Well-formed: equal dimensions, every edge in range.
    fn valid(&self) -> bool {
        let n = self.n();
        let in_range = |g: &SparseGraph| g.edges.iter().all(|&(r, c, _)| r < n && c < n);
        n > 0
            && self.sym.n == n
            && self.degree.len() == n
            && self.teleport.len() == n
            && in_range(self.transition)
            && in_range(self.sym)
    }

    /// Commitment to every input of the computation except φ₀.
    pub fn commitment(&self) -> [u8; 32] {
        let p = self.params;
        let params = field_bytes(&[p.lambda_d, p.lambda_s, p.lambda_h, p.alpha]);
        digest(
            GRAPH_DOMAIN,
            &[
                &self.transition.commitment(),
                &self.sym.commitment(),
                &field_bytes(self.degree),
                &field_bytes(self.teleport),
                &params,
            ],
        )
    }
}

fn spmv(g: &SparseGraph, x: &[F]) -> Vec<F> {
    let mut y = vec![F::ZERO; g.n];
    for &(r, c, w) in &g.edges {
        y[r] += w * x[c];
    }
    y
}

/// One tri-kernel step: diffusion, springs, heat, combine, L1-normalize.
pub fn trikernel_step(phi: &[F], g: &PhiGraph<'_>) -> Vec<F> {
    let n = phi.len();
    let p = g.params;
    let tphi = spmv(g.transition, phi);
    let wphi = spmv(g.sym, phi);
    let h2 = spmv(g.sym, &wphi);
    let one_minus = F::ONE - p.alpha;
    let mut raw = vec![F::ZERO; n];
    let mut sum = F::ZERO;
    for i in 0..n {
        let d = p.alpha * g.teleport[i] + one_minus * tphi[i];
        let deg = g.degree[i];
        let s = if deg == F::ZERO { phi[i] } else { wphi[i] * deg.inv() };
        let d2 = deg * deg;
        let h = if d2 == F::ZERO { phi[i] } else { h2[i] * d2.inv() };
        raw[i] = p.lambda_d * d + p.lambda_s * s + p.lambda_h * h;
        sum += raw[i];
    }
    if sum == F::ZERO {
        return g.teleport.to_vec();
    }
    let inv = sum.inv();
    raw.iter().map(|v| *v * inv).collect()
}

/// Run `iterations` steps from φ₀ and bind the result.
pub fn compute_phi_star(g: &PhiGraph<'_>, phi0: &[F], iterations: u32) -> Option<PhiClaim> {
    if !g.valid() || phi0.len() != g.n() {
        return None;
    }
    let mut phi = phi0.to_vec();
    for _ in 0..iterations {
        phi = trikernel_step(&phi, g);
    }
    Some(PhiClaim {
        binding: PhiBinding {
            graph_commit: g.commitment(),
            phi0_hash: vector_hash(phi0),
            phi_star_hash: vector_hash(&phi),
            iterations,
        },
        phi_star: phi,
    })
}

/// Recompute and compare: the graphs must match the bound commitment, φ₀
/// its hash, and `iterations` steps from φ₀ must give the claimed φ*.
pub fn verify_phi_claim(binding: &PhiBinding, g: &PhiGraph<'_>, phi0: &[F]) -> bool {
    match compute_phi_star(g, phi0, binding.iterations) {
        Some(claim) => claim.binding == *binding,
        None => false,
    }
}

#[cfg(test)]
#[path = "phi_tests.rs"]
mod tests;
