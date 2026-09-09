// ---
// tags: foculus, rust, zheng
// crystal-type: source
// crystal-domain: cyber
// ---
//! One add row of zheng's universal step CCS, used as a fold step that
//! carries hashed material (tip heights, settlement tickets, pay intents).
//!
//! The row is `r0 = 5, r6 = r4 + r5` with `(r4, r5)` read off
//! `hemera(material)`. What the fold then proves: knowledge of satisfying
//! rows folded into one HyperNova accumulator, decided under a Statement.
//! What it does not prove: that `(r4, r5)` came from the material — that
//! derivation runs outside the constraint system, so the material is bound
//! only through the Statement the decider absorbs, never through the
//! witness (zheng CHANGELOG 0.3.2, "satisfying but meaningless witness").
//! Binding the material itself needs hash rows in the fold, i.e. a nox
//! trace — the recursion milestone, tracked in zheng.

use cyber_hemera::hash as hemera_hash;
use nebu::Goldilocks;
use zheng::ccs::{universal_ccs, universal_witness};
use zheng::types::{Accumulator, CCSInstance, CCSWitness, Proof, ProofGroup, Statement, TraceProof};
use zheng::{ProofParams, verify};

/// nox pattern tag of the add row.
const ADD: u64 = 5;

/// The universal step instance every foculus fold step belongs to.
pub(crate) fn instance() -> &'static CCSInstance {
    universal_ccs()
}

/// An empty universal-step accumulator.
pub(crate) fn blank_acc() -> Accumulator {
    Accumulator::blank(instance())
}

/// The add-row witness of `material`: `r4, r5` are the first two 8-byte
/// limbs of `hemera(material)`, `r6` their field sum; row t+1 is all zero.
pub(crate) fn add_step(material: &[u8]) -> CCSWitness {
    let h = hemera_hash(material);
    let bytes = h.as_bytes();
    let a = Goldilocks::new(u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0u8; 8])));
    let b = Goldilocks::new(u64::from_le_bytes(bytes[8..16].try_into().unwrap_or([0u8; 8])));
    let mut regs_t = [Goldilocks::ZERO; 16];
    regs_t[0] = Goldilocks::new(ADD);
    regs_t[4] = a;
    regs_t[5] = b;
    regs_t[6] = a + b;
    universal_witness(&regs_t, &[Goldilocks::ZERO; 16], None)
}

/// Verify a decided accumulator as a one-group zheng proof against
/// `statement` — the same check `zheng::verify` runs on a joy artifact.
pub(crate) fn verify_group(acc: &Accumulator, proof: &Proof, statement: &Statement) -> bool {
    if acc.step_count() == 0 {
        return false;
    }
    let trace_proof = TraceProof {
        universal: ProofGroup { proof: proof.clone(), accumulator: acc.clone() },
        binding: None,
    };
    verify(&trace_proof, statement, &ProofParams::default()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_step_satisfies_the_universal_instance() {
        assert!(instance().is_satisfied_by(&add_step(b"material")));
    }

    #[test]
    fn different_material_gives_different_rows() {
        assert_ne!(add_step(b"a").z, add_step(b"b").z);
    }
}
