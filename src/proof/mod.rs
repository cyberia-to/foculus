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
//!    alone ([`Relation::instance`], [`Relation::public`]). Two profiles
//!    prove it: zheng's public certificate v3 (the free witness positions
//!    are disclosed and every row is checked exactly) and zheng's profile 1,
//!    the user-device succinct profile (the free half committed, Spartan
//!    over Fp3 plus one WHIR opening, [`prove_succinct`] with a held
//!    [`RelationKey`]). Profile 1 is admitted only for a *closed* relation
//!    ([`Relation::closed`]): one whose statement no native witness check
//!    completes, since a committed witness cannot be read back.
//! 3. witness checks no CCS expresses yet ([`Relation::check_witness`]),
//!    run natively on the disclosed witness. Each one is a named gap a
//!    succinct profile must close with a sub-relation.
//!
//! The public profile is sound and deterministic, linear in the witness,
//! not succinct and not zero-knowledge. Profile 1 is sound (≥ 128 proven
//! bits, zheng `device_params`), succinct in the witness, not
//! zero-knowledge. Which statement uses which is a measured choice
//! (`audit/device-profile-choice-2026-10.md`). Nothing here calls zheng's
//! legacy commit / fold / decide / verify path, whose fold the verifier
//! never checked (zheng `specs/decider.md`).

mod r1cs;

pub(crate) use r1cs::{Lin, R1cs};

use nebu::Goldilocks as F;
use nebu::field::P;
use zheng::envelope::{AnySuccinct, succinct_proof_bytes};
use zheng::execution::Certificate;
use zheng::execution::certificate;
use zheng::execution::succinct::{self, SuccinctProof, Whir, WhirParams};
use zheng::types::{CCSInstance, CCSWitness};

/// Profile byte of the public certificate (zheng certificate v3).
pub const PROFILE_PUBLIC: u8 = 1;
/// Profile byte of zheng's profile 1 (one relation, committed witness).
pub const PROFILE_SUCCINCT: u8 = 2;

/// A proof of one [`Relation`] instance.
///
/// One enum so a succinct or accumulated profile slots in as a new variant
/// without touching callers: they hold `Proof` and call [`verify`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proof {
    /// zheng public certificate v3: disclosed witness, every row checked.
    Public(PublicProof),
    /// zheng profile 1: committed witness, Spartan + one WHIR opening.
    Succinct(SuccinctBody),
}

/// Profile-1 body: the statement digest and zheng's succinct proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuccinctBody {
    pub statement: [u8; 32],
    pub proof: Box<SuccinctProof<Whir>>,
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
            Proof::Succinct(_) => PROFILE_SUCCINCT,
        }
    }

    /// The statement digest this proof is bound to.
    pub fn statement(&self) -> &[u8; 32] {
        match self {
            Proof::Public(p) => &p.statement,
            Proof::Succinct(p) => &p.statement,
        }
    }

    /// Bytes on the wire: profile ‖ statement ‖ body. Public: u32 count ‖
    /// u64 values (the encoding of `signal_codec`). Succinct: zheng's
    /// profile-1 proof bytes (PCS id, parameters, commitment, Spartan
    /// messages, opening).
    pub fn wire_size(&self) -> usize {
        match self {
            Proof::Public(p) => 1 + 32 + 4 + 8 * p.certificate.free.len(),
            Proof::Succinct(p) => 1 + 32 + succinct_bytes(&p.proof).len(),
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
    /// The relation and its pins are the whole witness statement: no
    /// [`Relation::check_witness`] gap. Only a closed relation may be proven
    /// with the witness committed (profile 1); the default is `false`, so a
    /// relation with a native check can never be proven succinctly by
    /// omission.
    fn closed(&self) -> bool {
        false
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
    /// Profile 1 asked of a relation that is not [`Relation::closed`], or a
    /// key derived for another relation shape.
    NotClosed,
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
        Proof::Succinct(p) => {
            if p.statement != relation.statement() {
                return Err(ProofError::WrongStatement);
            }
            let key = RelationKey::new(relation)?;
            key.verify(relation, p)
        }
    }
}

/// [`verify`] with a held key for the relation's shape: the relation is not
/// recompiled for a profile-1 proof. A key of another shape is refused.
pub fn verify_with_key<R: Relation>(key: &RelationKey, relation: &R, proof: &Proof) -> Result<(), ProofError> {
    match proof {
        Proof::Public(_) => verify(relation, proof),
        Proof::Succinct(p) => {
            if p.statement != relation.statement() {
                return Err(ProofError::WrongStatement);
            }
            key.verify(relation, p)
        }
    }
}

const SUCCINCT_DOMAIN: &[u8] = b"foculus-succinct-v1";

/// What a device holds to prove (and a verifier to check) one relation
/// shape in profile 1 many times: the compiled relation, the pinned
/// positions, the relation's digest (absorbed by the transcript, so a proof
/// is bound to the exact relation as well as the statement) and the WHIR
/// parameters zheng's `device_params` picks for its committed size.
///
/// zheng holds the column layout and the relabelled relation inside its own
/// `ProvingKey` for nox programs only; for a caller-built CCS it re-derives
/// them per proof (`succinct::prove_relation`). That cost is measured in
/// the audit; holding it too needs a zheng API (`ProvingKey` over a CCS).
pub struct RelationKey {
    instance: CCSInstance,
    positions: Vec<usize>,
    digest: [u8; 32],
    vars: usize,
    params: WhirParams,
}

impl RelationKey {
    /// Derive the key from a relation of the shape to be proven. Refuses a
    /// relation that is not [`Relation::closed`].
    pub fn new<R: Relation>(relation: &R) -> Result<Self, ProofError> {
        if !relation.closed() {
            return Err(ProofError::NotClosed);
        }
        let instance = relation.instance();
        let pins = with_constant(relation.public());
        let vars = succinct::committed_vars(&instance, &pins).map_err(|_| ProofError::Rejected)?;
        Ok(Self {
            digest: instance_digest(&instance),
            positions: pins.iter().map(|&(i, _)| i).collect(),
            instance,
            vars,
            params: succinct::device_params(vars),
        })
    }

    /// Committed variables of the relation (`ℓ`).
    pub fn vars(&self) -> usize {
        self.vars
    }

    /// The relation's pins, `(0, 1)` first, if `relation` has this key's
    /// shape. The instance is compared by digest only when the caller did
    /// not hold the key for this exact shape (positions differ → refused).
    fn pins<R: Relation>(&self, relation: &R) -> Result<Vec<(usize, F)>, ProofError> {
        if !relation.closed() {
            return Err(ProofError::NotClosed);
        }
        let pins = with_constant(relation.public());
        if pins.len() != self.positions.len() || pins.iter().zip(&self.positions).any(|(&(i, _), &j)| i != j) {
            return Err(ProofError::NotClosed);
        }
        Ok(pins)
    }

    fn transcript(&self, statement: &[u8; 32]) -> Vec<u8> {
        let mut bytes = SUCCINCT_DOMAIN.to_vec();
        bytes.extend_from_slice(&self.digest);
        bytes.extend_from_slice(statement);
        bytes
    }

    fn verify<R: Relation>(&self, relation: &R, body: &SuccinctBody) -> Result<(), ProofError> {
        let pins = self.pins(relation)?;
        succinct::verify_relation::<Whir>(&self.instance, &pins, &self.transcript(&body.statement), &body.proof)
            .map_err(|_| ProofError::Rejected)
    }
}

/// Prove `relation` in profile 1 with a held key of its shape.
pub fn prove_succinct<R: Witnessed>(key: &RelationKey, relation: &R) -> Result<Proof, ProofError> {
    let pins = key.pins(relation)?;
    let witness = relation.witness().ok_or(ProofError::Unsatisfied)?;
    if witness.z.len() != key.instance.num_cols || !key.instance.is_satisfied_by(&witness) {
        return Err(ProofError::Unsatisfied);
    }
    let statement = relation.statement();
    let proof = succinct::prove_relation::<Whir>(&key.params, &key.instance, &witness.z, &pins, &key.transcript(&statement))
        .map_err(|_| ProofError::Unsatisfied)?;
    Ok(Proof::Succinct(SuccinctBody { statement, proof: Box::new(proof) }))
}

/// zheng's profile-1 proof bytes (no statement).
pub fn succinct_bytes(proof: &SuccinctProof<Whir>) -> Vec<u8> {
    succinct_proof_bytes(&AnySuccinct::from(proof.clone()))
}

fn with_constant(public: Vec<(usize, F)>) -> Vec<(usize, F)> {
    core::iter::once((0, F::ONE)).chain(public).collect()
}

/// hemera digest of the relation: dimensions, multisets, coefficients and
/// every matrix entry in row order.
fn instance_digest(instance: &CCSInstance) -> [u8; 32] {
    let mut h = cyber_hemera::Hasher::new();
    h.update(b"foculus-relation-v1");
    for n in [instance.num_rows, instance.num_cols, instance.matrices.len(), instance.multisets.len()] {
        h.update(&(n as u64).to_le_bytes());
    }
    for (set, c) in instance.multisets.iter().zip(&instance.coeffs) {
        h.update(&(set.len() as u64).to_le_bytes());
        for &m in set {
            h.update(&(m as u64).to_le_bytes());
        }
        h.update(&c.as_u64().to_le_bytes());
    }
    for m in &instance.matrices {
        for row in &m.entries {
            h.update(&(row.len() as u64).to_le_bytes());
            for &(c, v) in row {
                h.update(&(c as u64).to_le_bytes());
                h.update(&v.as_u64().to_le_bytes());
            }
        }
    }
    *h.finalize().as_bytes()
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
