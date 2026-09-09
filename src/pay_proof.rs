// ---
// tags: foculus, rust, pay, zheng
// crystal-type: source
// crystal-domain: cyber
// ---
//! Pay proofs — zheng σ for money Intents (WP depth: σ required on pay).
//!
//! A pay proof is a HyperNova fold + decide over one add row of the
//! universal step CCS ([`crate::step`]) whose registers hash the pay
//! content id and conservation claim. Light and full peers verify with
//! `verify_pay` without re-executing the wallet. Binding caveat in
//! [`crate::step`].
//!
//! Statement binding:
//! - program_hash = domain "foculus-pay-v0"
//! - input_hash   = hemera(content_id ‖ total_out ‖ leg_count)
//! - output_hash  = content_id (signal identity)
//! - focus_bound  = total_out (amount bound)

use cyber_hemera::hash as hemera_hash;
use zheng::types::{Accumulator, Proof, ProofParams, Statement};
use zheng::{Transcript, decide, fold};

use bbg::Particle;

use crate::step;

const PROGRAM: [u8; 32] = *b"foculus-pay-proof-v0...........\0";

/// Public inputs for a pay proof.
#[derive(Clone, Debug)]
pub struct PayStatement {
    /// Signal content_id (covers links + box_moves).
    pub content_id: Particle,
    /// Sum of pay leg amounts.
    pub total_out: u64,
    /// Number of legs.
    pub leg_count: u32,
}

impl PayStatement {
    pub fn to_zheng(&self) -> Statement {
        let mut input = [0u8; 32];
        let h = hemera_hash(&self.material());
        let b = h.as_bytes();
        input[..b.len().min(32)].copy_from_slice(&b[..b.len().min(32)]);
        Statement {
            program_hash: PROGRAM,
            input_hash: input,
            output_hash: self.content_id,
            focus_bound: self.total_out,
            // no look rows in a pay program: the no-state-read sentinel
            bbg_root: [0u8; 32],
        }
    }

    /// content_id ‖ total_out ‖ leg_count — the bytes both the statement's
    /// input hash and the fold step's registers are read from.
    fn material(&self) -> [u8; 44] {
        let mut buf = [0u8; 44];
        buf[..32].copy_from_slice(&self.content_id);
        buf[32..40].copy_from_slice(&self.total_out.to_le_bytes());
        buf[40..44].copy_from_slice(&self.leg_count.to_le_bytes());
        buf
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PayProofError {
    FoldFailed,
    DecideFailed,
    VerifyFailed,
    Empty,
}

/// Prove a pay Intent: fold one conservation-binding step, decide.
pub fn prove_pay(stmt: &PayStatement) -> Result<Proof, PayProofError> {
    let acc = pay_acc(stmt)?;
    decide(&acc, &stmt.to_zheng(), &ProofParams::default()).map_err(|_| PayProofError::DecideFailed)
}

/// Verify a pay proof against the public pay statement.
///
/// The prover is deterministic (Fiat-Shamir, one hashed step), so the
/// verifier re-folds the accumulator from the statement, runs zheng's
/// verifier on the given proof against it, and additionally requires the
/// proof to match what an honest prover emits for this statement.
pub fn verify_pay(proof: &Proof, stmt: &PayStatement) -> bool {
    let Ok(acc) = pay_acc(stmt) else {
        return false;
    };
    let Ok(expected) = decide(&acc, &stmt.to_zheng(), &ProofParams::default()) else {
        return false;
    };
    proof.commitment.as_bytes() == expected.commitment.as_bytes()
        && proof.eval_value == expected.eval_value
        && step::verify_group(&acc, proof, &stmt.to_zheng())
}

/// The one-step accumulator of a pay statement.
fn pay_acc(stmt: &PayStatement) -> Result<Accumulator, PayProofError> {
    if stmt.leg_count == 0 {
        return Err(PayProofError::Empty);
    }
    let mut acc = step::blank_acc();
    let witness = step::add_step(&stmt.material());
    let mut t = Transcript::new();
    fold(&mut acc, step::instance(), &witness, &mut t).map_err(|_| PayProofError::FoldFailed)?;
    Ok(acc)
}

/// Hash nullifiers for finality / content binding.
pub fn nullifier_set_hash(nullifiers: &[Particle]) -> Particle {
    let mut buf = Vec::with_capacity(nullifiers.len() * 32 + 8);
    buf.extend_from_slice(&(nullifiers.len() as u64).to_le_bytes());
    for n in nullifiers {
        buf.extend_from_slice(n);
    }
    let h = hemera_hash(&buf);
    *h.as_bytes().first_chunk::<32>().unwrap_or(&[0u8; 32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prove_and_verify_pay() {
        let stmt = PayStatement {
            content_id: [3u8; 32],
            total_out: 150,
            leg_count: 2,
        };
        let proof = prove_pay(&stmt).unwrap();
        assert!(verify_pay(&proof, &stmt));
    }

    #[test]
    fn verify_rejects_wrong_content() {
        let stmt = PayStatement {
            content_id: [3u8; 32],
            total_out: 150,
            leg_count: 2,
        };
        let proof = prove_pay(&stmt).unwrap();
        let bad = PayStatement {
            content_id: [4u8; 32],
            total_out: 150,
            leg_count: 2,
        };
        assert!(!verify_pay(&proof, &bad));
    }

    #[test]
    fn verify_rejects_wrong_amount() {
        let stmt = PayStatement {
            content_id: [3u8; 32],
            total_out: 150,
            leg_count: 2,
        };
        let proof = prove_pay(&stmt).unwrap();
        let bad = PayStatement {
            content_id: [3u8; 32],
            total_out: 151,
            leg_count: 2,
        };
        assert!(!verify_pay(&proof, &bad));
    }

    #[test]
    fn empty_pay_errors() {
        let stmt = PayStatement {
            content_id: [1u8; 32],
            total_out: 0,
            leg_count: 0,
        };
        assert_eq!(prove_pay(&stmt).unwrap_err(), PayProofError::Empty);
    }
}
