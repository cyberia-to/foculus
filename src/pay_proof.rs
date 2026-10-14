// ---
// tags: foculus, rust, pay, zheng
// crystal-type: source
// crystal-domain: cyber
// ---
//! Pay proofs — σ for money Intents (WP depth: σ required on pay).
//!
//! The statement of a pay signal `S` with legs `a_0..a_{L−1}` (the amounts
//! of its links, in order):
//!
//! ```text
//! content_id = S.content_id()                        statement check
//! a_j = S.links[j].amount                             relation, legs pinned
//! a_j < 2^62,  s_j = s_{j−1} + a_j < 2^62,  s_{−1}=0  relation (CCS)
//! s_{L−1} = total_out                                 relation, total pinned
//! ```
//!
//! The range checks make the field sum an integer sum: no leg or running
//! total can wrap the Goldilocks modulus, so `total_out` is the amount that
//! leaves. The legs are the signal's own link amounts, public on the wire,
//! so they are pinned coordinates: the verifier places them from the
//! signal it holds and the relation is closed (no native witness check).
//! The witness is the bits and the running sums. Both proof profiles are
//! sound for it; pay stays in the public profile by measurement
//! (`audit/device-profile-choice-2026-10.md`: at every leg count the v3
//! certificate is smaller to verify and to send than a profile-1 proof).

use nebu::Goldilocks as F;
use zheng::types::{CCSInstance, CCSWitness};

use bbg::Particle;

use crate::chain::Signal;
use crate::proof::{self, Proof, R1cs, Relation, Witnessed, digest};

const STATEMENT_DOMAIN: &[u8] = b"foculus-pay-proof-v1";

/// Bits of every leg and running total: amounts are below 2^62, so a sum of
/// two never reaches the field modulus.
pub const AMOUNT_BITS: usize = 62;
/// Largest leg or total a pay proof accepts.
pub const MAX_PAY_AMOUNT: u64 = (1 << AMOUNT_BITS) - 1;

/// Public inputs of a pay proof, derived from the signal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayStatement {
    /// Signal content_id (covers links + box_moves).
    pub content_id: Particle,
    /// Sum of pay leg amounts.
    pub total_out: u64,
    /// Number of legs.
    pub leg_count: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PayProofError {
    /// A signal without links has nothing to pay.
    Empty,
    /// A leg or the total is above [`MAX_PAY_AMOUNT`].
    Overflow,
    Proof(proof::ProofError),
}

impl std::fmt::Display for PayProofError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("pay proof over a signal without links"),
            Self::Overflow => f.write_str("pay leg or total above the pay proof range"),
            Self::Proof(e) => write!(f, "pay proof rejected: {e:?}"),
        }
    }
}

impl std::error::Error for PayProofError {}

impl PayStatement {
    /// The statement of a pay signal: its content id, leg count and total.
    pub fn of(signal: &Signal) -> Result<Self, PayProofError> {
        if signal.links.is_empty() {
            return Err(PayProofError::Empty);
        }
        let mut total = 0u64;
        for l in &signal.links {
            total = total
                .checked_add(l.amount)
                .filter(|&t| l.amount <= MAX_PAY_AMOUNT && t <= MAX_PAY_AMOUNT)
                .ok_or(PayProofError::Overflow)?;
        }
        Ok(Self {
            content_id: signal.content_id(),
            total_out: total,
            leg_count: u32::try_from(signal.links.len()).map_err(|_| PayProofError::Overflow)?,
        })
    }
}

/// The relation of one pay signal.
pub struct PayRelation<'a> {
    signal: &'a Signal,
    statement: PayStatement,
}

impl<'a> PayRelation<'a> {
    /// The relation a verifier derives from `signal`.
    pub fn of(signal: &'a Signal) -> Result<Self, PayProofError> {
        Ok(Self { signal, statement: PayStatement::of(signal)? })
    }
}

/// Columns per leg: amount, its bits, running total, its bits.
const PER_LEG: usize = 2 * (1 + AMOUNT_BITS);
const TOTAL: usize = 1;

fn leg_amount(j: usize) -> usize {
    2 + j * PER_LEG
}
fn leg_sum(j: usize) -> usize {
    leg_amount(j) + 1 + AMOUNT_BITS
}

fn bits_after(col: usize) -> Vec<usize> {
    (col + 1..=col + AMOUNT_BITS).collect()
}

impl Relation for PayRelation<'_> {
    fn statement(&self) -> [u8; 32] {
        let s = &self.statement;
        digest(
            STATEMENT_DOMAIN,
            &[&s.content_id, &s.total_out.to_le_bytes(), &s.leg_count.to_le_bytes()],
        )
    }

    fn instance(&self) -> CCSInstance {
        let legs = self.statement.leg_count as usize;
        let mut cs = R1cs::new(2 + legs * PER_LEG);
        for j in 0..legs {
            cs.packs(leg_amount(j), &bits_after(leg_amount(j)));
            cs.packs(leg_sum(j), &bits_after(leg_sum(j)));
            let mut step = vec![(leg_sum(j), F::ONE), (leg_amount(j), -F::ONE)];
            if j > 0 {
                step.push((leg_sum(j - 1), -F::ONE));
            }
            cs.zero(step);
        }
        cs.zero(vec![(leg_sum(legs - 1), F::ONE), (TOTAL, -F::ONE)]);
        cs.instance()
    }

    fn public(&self) -> Vec<(usize, F)> {
        core::iter::once((TOTAL, F::new(self.statement.total_out)))
            .chain(self.signal.links.iter().enumerate().map(|(j, l)| (leg_amount(j), F::new(l.amount))))
            .collect()
    }

    fn closed(&self) -> bool {
        true
    }
}

impl Witnessed for PayRelation<'_> {
    fn witness(&self) -> Option<CCSWitness> {
        let mut z = vec![F::ZERO; self.instance().num_cols];
        z[0] = F::ONE;
        z[TOTAL] = F::new(self.statement.total_out);
        let mut sum = 0u64;
        for (j, l) in self.signal.links.iter().enumerate() {
            sum = sum.checked_add(l.amount)?;
            for (col, value) in [(leg_amount(j), l.amount), (leg_sum(j), sum)] {
                z[col] = F::new(value);
                for (k, bit) in bits_after(col).into_iter().enumerate() {
                    z[bit] = F::new((value >> k) & 1);
                }
            }
        }
        Some(CCSWitness { z })
    }
}

/// Prove a pay signal's conservation claim. Attach the result as
/// `signal.proof`; `content_id` does not cover the proof.
pub fn prove_pay(signal: &Signal) -> Result<Proof, PayProofError> {
    let statement = PayStatement::of(signal)?;
    proof::prove(&PayRelation { signal, statement }).map_err(PayProofError::Proof)
}

/// Verify a pay proof against the signal it pays for.
pub fn verify_pay(proof: &Proof, signal: &Signal) -> bool {
    check_pay(proof, signal).is_ok()
}

/// [`verify_pay`] with the reason a proof is rejected.
pub fn check_pay(proof: &Proof, signal: &Signal) -> Result<(), PayProofError> {
    let statement = PayStatement::of(signal)?;
    proof::verify(&PayRelation { signal, statement }, proof).map_err(PayProofError::Proof)
}

/// Admission check of a signal's proof field. A signal without a proof
/// passes; a signal with one must carry a valid pay proof of itself — the
/// only statement a signal proof makes today.
pub fn check_signal_proof(signal: &Signal) -> Result<(), PayProofError> {
    match &signal.proof {
        None => Ok(()),
        Some(proof) => check_pay(proof, signal),
    }
}

/// Hash nullifiers for finality / content binding.
pub fn nullifier_set_hash(nullifiers: &[Particle]) -> Particle {
    let mut buf = Vec::with_capacity(nullifiers.len() * 32 + 8);
    buf.extend_from_slice(&(nullifiers.len() as u64).to_le_bytes());
    for n in nullifiers {
        buf.extend_from_slice(n);
    }
    let h = cyber_hemera::hash(&buf);
    *h.as_bytes().first_chunk::<32>().unwrap_or(&[0u8; 32])
}

#[cfg(test)]
#[path = "pay_proof_tests.rs"]
mod tests;
