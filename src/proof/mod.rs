// ---
// tags: foculus, rust, zheng, proof
// crystal-type: source
// crystal-domain: cyber
// ---
//! The one proof type foculus carries: settlement tickets, pay intents and
//! signals all hold a [`Proof`] over a [`Relation`] the verifier derives.
//!
//! Three kinds of check make up every statement:
//!
//! 1. statement checks — hashes, orderings, win tests, chain links over
//!    public data. The verifier recomputes them in every profile; a proof
//!    of a one-hash computation would cost more than the hash.
//! 2. the witness relation — a CCS the verifier builds from public data
//!    alone ([`Relation::instance`], [`Relation::public`]). Today it is
//!    proven by zheng's public certificate v3: the free witness positions
//!    are disclosed and every row is checked exactly. A later succinct or
//!    accumulated profile replaces the certificate, not the relation.
//! 3. witness checks no CCS expresses yet ([`Relation::check_witness`]),
//!    run natively on the disclosed witness. Each one is a named gap a
//!    succinct profile must close with a sub-relation.
//!
//! The public profile is sound and deterministic, linear in the witness,
//! not succinct and not zero-knowledge. Nothing here calls zheng's legacy
//! commit / fold / decide / verify path, whose fold the verifier never
//! checked (zheng `specs/decider.md`).

mod r1cs;

pub(crate) use r1cs::{Lin, R1cs};

use nebu::Goldilocks as F;
use nebu::field::P;
use zheng::execution::Certificate;
use zheng::execution::certificate;
use zheng::types::{CCSInstance, CCSWitness};

/// Profile byte of the public certificate (zheng certificate v3).
pub const PROFILE_PUBLIC: u8 = 1;

/// A proof of one [`Relation`] instance.
///
/// One enum so a succinct or accumulated profile slots in as a new variant
/// without touching callers: they hold `Proof` and call [`verify`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proof {
    /// zheng public certificate v3: disclosed witness, every row checked.
    Public(PublicProof),
}

/// Public-profile body: the statement digest the proof is bound to and the
/// v3 certificate (free witness positions, trailing zeros trimmed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicProof {
    pub statement: [u8; 32],
    pub certificate: Certificate,
}

impl Proof {
    pub fn profile(&self) -> u8 {
        match self {
            Proof::Public(_) => PROFILE_PUBLIC,
        }
    }

    /// The statement digest this proof is bound to.
    pub fn statement(&self) -> &[u8; 32] {
        match self {
            Proof::Public(p) => &p.statement,
        }
    }

    /// Bytes on the wire (profile ‖ statement ‖ u32 count ‖ u64 values),
    /// the encoding of `signal_codec`.
    pub fn wire_size(&self) -> usize {
        match self {
            Proof::Public(p) => 1 + 32 + 4 + 8 * p.certificate.free.len(),
        }
    }
}

/// A statement whose relation the verifier derives from public data.
pub trait Relation {
    /// Domain-separated digest of the public statement. A proof carries it,
    /// so a proof made for one statement is rejected for any other.
    fn statement(&self) -> [u8; 32];
    /// The constraint system, from public data only.
    fn instance(&self) -> CCSInstance;
    /// Pinned public coordinates: strictly increasing, excluding z[0] = 1.
    fn public(&self) -> Vec<(usize, F)>;
    /// Checks on the disclosed witness that no CCS expresses yet.
    fn check_witness(&self, _z: &[F]) -> bool {
        true
    }
}

/// A relation whose prover can build a satisfying witness.
pub trait Witnessed: Relation {
    fn witness(&self) -> Option<CCSWitness>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofError {
    /// The prover's witness does not satisfy the relation or its pins.
    Unsatisfied,
    /// The proof was made for another statement.
    WrongStatement,
    /// The certificate does not satisfy the derived relation.
    Rejected,
    /// The disclosed witness fails a native check (see [`Relation::check_witness`]).
    WitnessCheck,
}

/// Prove `relation` in the public profile.
pub fn prove<R: Witnessed>(relation: &R) -> Result<Proof, ProofError> {
    let witness = relation.witness().ok_or(ProofError::Unsatisfied)?;
    let instance = relation.instance();
    let public = relation.public();
    let certificate = certificate::certify(&instance, &witness, &public)
        .map_err(|_| ProofError::Unsatisfied)?;
    Ok(Proof::Public(PublicProof {
        statement: relation.statement(),
        certificate,
    }))
}

/// Verify `proof` against the relation the caller derived.
pub fn verify<R: Relation>(relation: &R, proof: &Proof) -> Result<(), ProofError> {
    match proof {
        Proof::Public(p) => {
            if p.statement != relation.statement() {
                return Err(ProofError::WrongStatement);
            }
            let instance = relation.instance();
            let public = relation.public();
            certificate::verify(&instance, &p.certificate, &public)
                .map_err(|_| ProofError::Rejected)?;
            // The witness the native checks read is placed here and checked
            // against the same rows, so those checks never depend on how the
            // certificate format is assembled elsewhere.
            let z = assemble(&instance, &p.certificate, &public).ok_or(ProofError::Rejected)?;
            if !instance.is_satisfied_by(&CCSWitness { z: z.clone() }) {
                return Err(ProofError::Rejected);
            }
            if !relation.check_witness(&z) {
                return Err(ProofError::WitnessCheck);
            }
            Ok(())
        }
    }
}

/// Place z[0] = 1 and the pins, fill the remaining positions in index order
/// from the certificate, zero the rest.
fn assemble(instance: &CCSInstance, cert: &Certificate, public: &[(usize, F)]) -> Option<Vec<F>> {
    let mut z = vec![F::ZERO; instance.num_cols];
    let mut pinned = vec![false; instance.num_cols];
    *z.first_mut()? = F::ONE;
    *pinned.first_mut()? = true;
    for &(i, v) in public {
        *z.get_mut(i)? = v;
        pinned[i] = true;
    }
    let mut free = cert.free.iter();
    for (i, slot) in z.iter_mut().enumerate() {
        if pinned[i] {
            continue;
        }
        match free.next() {
            Some(&v) if v < P => *slot = F::new(v),
            Some(_) => return None,
            None => break,
        }
    }
    if free.next().is_some() {
        return None;
    }
    Some(z)
}

/// Domain-separated hemera digest, 32 bytes.
pub(crate) fn digest(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut h = cyber_hemera::Hasher::new();
    h.update(&(domain.len() as u64).to_le_bytes());
    h.update(domain);
    for part in parts {
        h.update(&(part.len() as u64).to_le_bytes());
        h.update(part);
    }
    *h.finalize().as_bytes()
}

#[cfg(test)]
mod tests;
