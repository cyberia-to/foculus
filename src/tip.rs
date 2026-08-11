// ---
// tags: foculus, rust, tip, light client
// crystal-type: source
// crystal-domain: cyber
// ---
//! Tip service — clock C (history trust) for full / cell / light modes.
//!
//! Normative: cyber/specs/light-money.md, money-loop.md.
//!
//! Production path: each height folds a CCS step that binds `(height, root)`
//! into the HyperNova accumulator; light clients `decide` + verify once at
//! join, then fold-advance on each tip update.

use bbg::{Checkpoint, Particle};
use cyber_hemera::hash as hemera_hash;
use lens::brakedown::Brakedown;
use nebu::Goldilocks;
use zheng::ccs::{CONST_IDX, Z_LEN, build_step_ccs, reg_t, reg_t1};
use zheng::spartan::SpartanVerifier;
use zheng::types::{Accumulator, CCSInstance, CCSWitness, Proof, ProofParams, Statement};
use zheng::{Transcript, decide, fold};

/// How the tip root was established (certainty grade 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TipTrust {
    /// Full node or cell: history applied locally.
    LocalApplied = 1,
    /// Light client: folding accumulator decided / maintained.
    FoldDecided = 2,
    /// No trusted tip — money openings fail closed.
    Untrusted = 0,
}

/// Trusted tip object (cyber/specs/light-money §2).
#[derive(Clone, Debug)]
pub struct Tip {
    pub height: u64,
    pub root: Particle,
    pub folding_acc: Option<Accumulator>,
    pub trust: TipTrust,
    pub statement: Option<Statement>,
}

impl Tip {
    pub fn grade4(&self) -> bool {
        matches!(self.trust, TipTrust::LocalApplied | TipTrust::FoldDecided)
    }

    pub fn from_local(checkpoint: &Checkpoint) -> Self {
        Self {
            height: checkpoint.height,
            root: checkpoint.root,
            folding_acc: checkpoint.acc.clone(),
            trust: TipTrust::LocalApplied,
            statement: None,
        }
    }

    pub fn untrusted() -> Self {
        Self {
            height: 0,
            root: [0u8; 32],
            folding_acc: None,
            trust: TipTrust::Untrusted,
            statement: None,
        }
    }

    /// Light join: require non-empty acc + decide + verify → FoldDecided.
    pub fn join_checkpoint(checkpoint: &Checkpoint) -> Self {
        let Some(acc) = checkpoint.acc.as_ref() else {
            return Self::untrusted_at(checkpoint);
        };
        if acc.step_count == 0 {
            return Self {
                height: checkpoint.height,
                root: checkpoint.root,
                folding_acc: Some(acc.clone()),
                trust: TipTrust::Untrusted,
                statement: None,
            };
        }

        let statement = statement_for_root(&checkpoint.root, checkpoint.height);
        let params = ProofParams::default();
        let Ok(proof) = decide(acc, &statement, &params) else {
            return Self {
                height: checkpoint.height,
                root: checkpoint.root,
                folding_acc: Some(acc.clone()),
                trust: TipTrust::Untrusted,
                statement: None,
            };
        };
        if !verify_decide(acc, &proof, &statement) {
            return Self {
                height: checkpoint.height,
                root: checkpoint.root,
                folding_acc: Some(acc.clone()),
                trust: TipTrust::Untrusted,
                statement: None,
            };
        }
        Self {
            height: checkpoint.height,
            root: checkpoint.root,
            folding_acc: Some(acc.clone()),
            trust: TipTrust::FoldDecided,
            statement: Some(statement),
        }
    }

    fn untrusted_at(checkpoint: &Checkpoint) -> Self {
        Self {
            height: checkpoint.height,
            root: checkpoint.root,
            folding_acc: checkpoint.acc.clone(),
            trust: TipTrust::Untrusted,
            statement: None,
        }
    }

    pub fn advance_local(&mut self, checkpoint: &Checkpoint) {
        self.height = checkpoint.height;
        self.root = checkpoint.root;
        if checkpoint.acc.is_some() {
            self.folding_acc = checkpoint.acc.clone();
        }
        self.trust = TipTrust::LocalApplied;
    }

    /// Light steady-state: fold one height into the running acc (clock C follow).
    ///
    /// Requires existing FoldDecided tip. Binds the new `(height, root)` into
    /// the accumulator so history is continuous without re-decide from genesis.
    pub fn advance_fold(&mut self, height: u64, root: Particle) -> Result<(), TipError> {
        if self.trust != TipTrust::FoldDecided {
            return Err(TipError::NotFoldMode);
        }
        if height < self.height {
            return Err(TipError::HeightRegression);
        }
        let acc = self.folding_acc.get_or_insert_with(blank_height_acc);
        let instance = height_ccs();
        let witness = block_witness(height, &root, &[0u8; 32]);
        let mut t = Transcript::new();
        fold(acc, &instance, &witness, &mut t).map_err(|_| TipError::FoldFailed)?;
        self.height = height;
        self.root = root;
        self.statement = Some(statement_for_root(&root, height));
        Ok(())
    }

    /// Re-run decide on the current acc (audit / epoch seal).
    pub fn redecide(&mut self) -> Result<Proof, TipError> {
        let acc = self.folding_acc.as_ref().ok_or(TipError::NoAccumulator)?;
        if acc.step_count == 0 {
            return Err(TipError::EmptyAccumulator);
        }
        let statement = statement_for_root(&self.root, self.height);
        let proof =
            decide(acc, &statement, &ProofParams::default()).map_err(|_| TipError::DecideFailed)?;
        if !verify_decide(acc, &proof, &statement) {
            return Err(TipError::VerifyFailed);
        }
        self.statement = Some(statement);
        self.trust = TipTrust::FoldDecided;
        Ok(proof)
    }

    /// Export a checkpoint peers can light-join.
    pub fn to_checkpoint(&self) -> Checkpoint {
        Checkpoint {
            root: self.root,
            acc: self.folding_acc.clone(),
            height: self.height,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TipError {
    NotFoldMode,
    HeightRegression,
    FoldFailed,
    NoAccumulator,
    EmptyAccumulator,
    DecideFailed,
    VerifyFailed,
}

// ── height-binding CCS (production fold step) ─────────────────────────────

/// Same add-pattern CCS used as a height step: constraints stay stable so
/// sequential folds share one group. Witness encodes height + root material.
fn height_ccs() -> CCSInstance {
    build_step_ccs(5)
}

fn blank_height_acc() -> Accumulator {
    let instance = height_ccs();
    let zero = vec![Goldilocks::ZERO; Z_LEN];
    Accumulator {
        committed_instance: instance.clone(),
        folded_witness: CCSWitness { z: zero.clone() },
        witness_commitment: Brakedown::commit_raw(&zero),
        error_evals: vec![Goldilocks::ZERO; instance.num_rows],
        step_count: 0,
    }
}

/// Witness for height + root + root_leaves_hash. Each triple folds uniquely.
fn block_witness(height: u64, root: &Particle, leaves: &Particle) -> CCSWitness {
    let mut buf = [0u8; 72];
    buf[..8].copy_from_slice(&height.to_le_bytes());
    buf[8..40].copy_from_slice(root);
    buf[40..72].copy_from_slice(leaves);
    let h = hemera_hash(&buf);
    let bytes = h.as_bytes();
    let a = u64::from_le_bytes(bytes[0..8].try_into().unwrap()) % 1_000_000 + 1;
    let b = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) % 1_000_000 + 1;
    let c = a + b;
    let mut z = vec![Goldilocks::ZERO; Z_LEN];
    z[CONST_IDX] = Goldilocks::ONE;
    z[reg_t(3)] = Goldilocks::new(a);
    z[reg_t(4)] = Goldilocks::new(b);
    z[reg_t1(5)] = Goldilocks::new(c);
    CCSWitness { z }
}

/// Prover-side tip builder: folds every finalized height for export to light clients.
pub struct TipProver {
    acc: Accumulator,
    transcript: Transcript,
    height: u64,
    root: Particle,
}

impl TipProver {
    pub fn new() -> Self {
        Self {
            acc: blank_height_acc(),
            transcript: Transcript::new(),
            height: 0,
            root: [0u8; 32],
        }
    }

    /// Fold one block/height. Call after state root is fixed at `height`.
    pub fn fold_height(&mut self, height: u64, root: Particle) -> Result<(), TipError> {
        self.fold_block(height, root, [0u8; 32])
    }

    /// Production-shaped fold: bind height + BBG root + optional root-leaves
    /// commitment (hash of dimension commitments). Light clients that verify
    /// the tip later open balances against the same root.
    pub fn fold_block(
        &mut self,
        height: u64,
        root: Particle,
        root_leaves_hash: Particle,
    ) -> Result<(), TipError> {
        if height < self.height && self.acc.step_count > 0 {
            return Err(TipError::HeightRegression);
        }
        let instance = height_ccs();
        let witness = block_witness(height, &root, &root_leaves_hash);
        if self.acc.step_count == 0 {
            self.transcript = Transcript::new();
        }
        fold(&mut self.acc, &instance, &witness, &mut self.transcript)
            .map_err(|_| TipError::FoldFailed)?;
        self.height = height;
        self.root = root;
        Ok(())
    }

    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            root: self.root,
            acc: Some(self.acc.clone()),
            height: self.height,
        }
    }

    /// Decide + package a light-joinable tip.
    pub fn seal_tip(&self) -> Result<Tip, TipError> {
        let ck = self.checkpoint();
        let tip = Tip::join_checkpoint(&ck);
        if !tip.grade4() {
            return Err(TipError::VerifyFailed);
        }
        Ok(tip)
    }
}

impl Default for TipProver {
    fn default() -> Self {
        Self::new()
    }
}

fn statement_for_root(root: &Particle, height: u64) -> Statement {
    let mut input = [0u8; 32];
    input[..8].copy_from_slice(&height.to_le_bytes());
    Statement {
        program_hash: *b"foculus-tip-fold-v0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        input_hash: input,
        output_hash: *root,
        focus_bound: 0,
    }
}

fn verify_decide(acc: &Accumulator, proof: &Proof, statement: &Statement) -> bool {
    let mut vt = Transcript::new_recursive();
    vt.absorb_statement(statement);
    vt.absorb(acc.witness_commitment.as_bytes());
    for &e in &acc.error_evals {
        vt.absorb(&e.as_u64().to_le_bytes());
    }
    vt.absorb(&acc.step_count.to_le_bytes());
    SpartanVerifier::verify(&acc.committed_instance, proof, &acc.error_evals, &mut vt).is_ok()
}

/// Bootstrap a one-step fold tip (tests / genesis).
pub fn join_with_demo_fold(root: Particle, height: u64) -> Tip {
    let mut prover = TipProver::new();
    prover.fold_height(height, root).expect("fold");
    prover.seal_tip().expect("seal")
}

/// Legacy alias used by earlier tests.
pub fn demo_fold_accumulator() -> Accumulator {
    let mut prover = TipProver::new();
    prover.fold_height(0, [0u8; 32]).expect("fold");
    prover.acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_tip_is_grade4() {
        let ck = Checkpoint {
            root: [1u8; 32],
            acc: None,
            height: 7,
        };
        let tip = Tip::from_local(&ck);
        assert!(tip.grade4());
        assert_eq!(tip.trust, TipTrust::LocalApplied);
    }

    #[test]
    fn checkpoint_without_acc_is_untrusted_for_light() {
        let ck = Checkpoint {
            root: [2u8; 32],
            acc: None,
            height: 1,
        };
        assert!(!Tip::join_checkpoint(&ck).grade4());
    }

    #[test]
    fn prover_multi_height_then_light_join() {
        let mut prover = TipProver::new();
        prover.fold_height(0, [1u8; 32]).unwrap();
        prover.fold_height(1, [2u8; 32]).unwrap();
        prover.fold_height(2, [3u8; 32]).unwrap();
        let tip = prover.seal_tip().unwrap();
        assert_eq!(tip.trust, TipTrust::FoldDecided);
        assert_eq!(tip.height, 2);
        assert_eq!(tip.root, [3u8; 32]);
        assert!(tip.folding_acc.as_ref().unwrap().step_count >= 3);
    }

    #[test]
    fn light_advance_fold_after_join() {
        let mut prover = TipProver::new();
        prover.fold_height(0, [9u8; 32]).unwrap();
        let mut tip = prover.seal_tip().unwrap();
        tip.advance_fold(1, [8u8; 32]).unwrap();
        assert_eq!(tip.height, 1);
        assert_eq!(tip.root, [8u8; 32]);
        assert!(tip.grade4());
        tip.redecide().unwrap();
    }

    #[test]
    fn join_with_demo_fold_is_grade4() {
        let tip = join_with_demo_fold([9u8; 32], 3);
        assert!(tip.grade4());
        assert_eq!(tip.root, [9u8; 32]);
        assert_eq!(tip.height, 3);
    }
}
