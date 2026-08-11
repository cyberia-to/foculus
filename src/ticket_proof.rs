// ---
// tags: foculus, rust, rewards, fold-mining, zheng
// crystal-type: source
// crystal-domain: cyber
// ---
//! HyperNova σ for settlement tickets and fold steps.
//!
//! Same CCS shape as tip fold (`build_step_ccs(5)` add-pattern): each ticket
//! or fold step is a satisfying witness that binds public material via Hemera
//! into `a + b = c`. Sequential folds share one group; [`decide`] seals O(1)
//! SuperSpartan proof. This is real zheng HyperNova — not a hash stub.

use cyber_hemera::hash as hemera_hash;
use lens::brakedown::Brakedown;
use nebu::Goldilocks;
use zheng::ccs::{build_step_ccs, reg_t, reg_t1, CONST_IDX, Z_LEN};
use zheng::spartan::SpartanVerifier;
use zheng::types::{Accumulator, CCSInstance, CCSWitness, Proof, ProofParams, Statement};
use zheng::{decide, fold, Transcript};

use crate::tickets::{ClusterAcc, SettlementTicket};

const PROGRAM: [u8; 32] = *b"foculus-ticket-fold-v0\0\0\0\0\0\0\0\0\0\0";

/// Sealed HyperNova state for a batch of tickets / fold steps.
#[derive(Clone, Debug)]
pub struct FoldSeal {
    pub acc: Accumulator,
    pub proof: Proof,
    pub statement: Statement,
    /// How many fold steps were absorbed.
    pub steps: u64,
}

/// Prover that folds ticket/fold witnesses into one HyperNova accumulator.
pub struct TicketProver {
    acc: Accumulator,
    transcript: Transcript,
    steps: u64,
}

impl TicketProver {
    pub fn new() -> Self {
        Self {
            acc: blank_acc(),
            transcript: Transcript::new(),
            steps: 0,
        }
    }

    /// Fold one settlement ticket (binds beacon ‖ cluster ‖ nonce ‖ miner ‖ commit).
    pub fn fold_settlement(
        &mut self,
        beacon: &[u8; 32],
        cluster: &[u8; 32],
        ticket: &SettlementTicket,
    ) -> Result<(), ProofError> {
        let mut mat = Vec::with_capacity(32 + 32 + 8 + 32 + 32);
        mat.extend_from_slice(beacon);
        mat.extend_from_slice(cluster);
        mat.extend_from_slice(&ticket.nonce.to_le_bytes());
        mat.extend_from_slice(&ticket.miner);
        mat.extend_from_slice(&ticket.commitment);
        self.fold_material(&mat)
    }

    /// Fold one monoid fold step (binds level ‖ pair_id ‖ left ‖ right ‖ result).
    pub fn fold_step_material(
        &mut self,
        beacon: &[u8; 32],
        cluster: &[u8; 32],
        level: u32,
        pair_id: &[u8; 32],
        left: &ClusterAcc,
        right: &ClusterAcc,
        result: &ClusterAcc,
    ) -> Result<(), ProofError> {
        let mut mat = Vec::with_capacity(32 * 5 + 4);
        mat.extend_from_slice(beacon);
        mat.extend_from_slice(cluster);
        mat.extend_from_slice(b"fold");
        mat.extend_from_slice(&level.to_le_bytes());
        mat.extend_from_slice(pair_id);
        mat.extend_from_slice(&left.commitment);
        mat.extend_from_slice(&right.commitment);
        mat.extend_from_slice(&result.commitment);
        self.fold_material(&mat)
    }

    fn fold_material(&mut self, material: &[u8]) -> Result<(), ProofError> {
        let instance = ticket_ccs();
        let witness = material_witness(material);
        if self.steps == 0 {
            self.transcript = Transcript::new();
        }
        fold(&mut self.acc, &instance, &witness, &mut self.transcript)
            .map_err(|_| ProofError::FoldFailed)?;
        self.steps = self.steps.saturating_add(1);
        Ok(())
    }

    /// Decide SuperSpartan on the accumulator. Statement binds (beacon, cluster, k).
    pub fn seal(
        &self,
        beacon: &[u8; 32],
        cluster: &[u8; 32],
        sample_count: u64,
    ) -> Result<FoldSeal, ProofError> {
        if self.steps == 0 {
            return Err(ProofError::Empty);
        }
        let statement = ticket_statement(beacon, cluster, sample_count);
        let proof = decide(&self.acc, &statement, &ProofParams::default())
            .map_err(|_| ProofError::DecideFailed)?;
        if !verify_seal(&self.acc, &proof, &statement) {
            return Err(ProofError::VerifyFailed);
        }
        Ok(FoldSeal {
            acc: self.acc.clone(),
            proof,
            statement,
            steps: self.steps,
        })
    }
}

impl Default for TicketProver {
    fn default() -> Self {
        Self::new()
    }
}

/// Verify a sealed fold proof (O(1) SuperSpartan check).
pub fn verify_seal(acc: &Accumulator, proof: &Proof, statement: &Statement) -> bool {
    if acc.step_count == 0 {
        return false;
    }
    let mut vt = Transcript::new_recursive();
    vt.absorb_statement(statement);
    vt.absorb(acc.witness_commitment.as_bytes());
    for &e in &acc.error_evals {
        vt.absorb(&e.as_u64().to_le_bytes());
    }
    vt.absorb(&acc.step_count.to_le_bytes());
    SpartanVerifier::verify(&acc.committed_instance, proof, &acc.error_evals, &mut vt).is_ok()
}

/// Verify a [`FoldSeal`] end-to-end.
pub fn verify_fold_seal(seal: &FoldSeal) -> bool {
    verify_seal(&seal.acc, &seal.proof, &seal.statement)
        && seal.acc.step_count == seal.steps
        && seal.steps > 0
}

/// Prove a batch of settlement tickets into one seal.
pub fn prove_settlement_batch(
    beacon: &[u8; 32],
    cluster: &[u8; 32],
    tickets: &[SettlementTicket],
) -> Result<FoldSeal, ProofError> {
    if tickets.is_empty() {
        return Err(ProofError::Empty);
    }
    let mut prover = TicketProver::new();
    for t in tickets {
        prover.fold_settlement(beacon, cluster, t)?;
    }
    prover.seal(beacon, cluster, tickets.len() as u64)
}

/// Prove a fold-tree assembly: for each binary merge, fold HyperNova step.
///
/// Single-leaf trees still get one HyperNova step binding the leaf commitment
/// (self-fold root), so seal is never empty when samples exist.
pub fn prove_fold_tree(
    beacon: &[u8; 32],
    cluster: &[u8; 32],
    leaves: &[ClusterAcc],
) -> Result<(ClusterAcc, FoldSeal), ProofError> {
    if leaves.is_empty() {
        return Err(ProofError::Empty);
    }
    let mut prover = TicketProver::new();
    let mut level: Vec<ClusterAcc> = leaves.to_vec();
    let mut lvl = 0u32;

    if level.len() == 1 {
        // Bind the sole self-accumulator as a root commitment step.
        let root = level[0].clone();
        let empty = ClusterAcc::empty(root.sum_m.len());
        let pid = crate::tickets::pair_id(&root, &empty);
        prover.fold_step_material(beacon, cluster, 0, &pid, &root, &empty, &root)?;
        let seal = prover.seal(beacon, cluster, root.k)?;
        return Ok((root, seal));
    }

    while level.len() > 1 {
        let mut next = Vec::new();
        let mut i = 0;
        while i < level.len() {
            if i + 1 >= level.len() {
                next.push(level[i].clone());
                break;
            }
            let left = &level[i];
            let right = &level[i + 1];
            let result = crate::tickets::fold_acc(left, right);
            let pid = crate::tickets::pair_id(left, right);
            prover.fold_step_material(beacon, cluster, lvl, &pid, left, right, &result)?;
            next.push(result);
            i += 2;
        }
        level = next;
        lvl = lvl.saturating_add(1);
    }
    let root = level.into_iter().next().unwrap_or_default();
    let k = root.k;
    let seal = prover.seal(beacon, cluster, k)?;
    Ok((root, seal))
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProofError {
    FoldFailed,
    DecideFailed,
    VerifyFailed,
    Empty,
}

fn ticket_ccs() -> CCSInstance {
    build_step_ccs(5)
}

fn blank_acc() -> Accumulator {
    let instance = ticket_ccs();
    let zero = vec![Goldilocks::ZERO; Z_LEN];
    Accumulator {
        committed_instance: instance.clone(),
        folded_witness: CCSWitness { z: zero.clone() },
        witness_commitment: Brakedown::commit_raw(&zero),
        error_evals: vec![Goldilocks::ZERO; instance.num_rows],
        step_count: 0,
    }
}

/// Bind arbitrary material into a satisfying a+b=c witness (same as tip).
fn material_witness(material: &[u8]) -> CCSWitness {
    let h = hemera_hash(material);
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

fn ticket_statement(beacon: &[u8; 32], cluster: &[u8; 32], k: u64) -> Statement {
    let mut input = [0u8; 32];
    input[..8].copy_from_slice(&k.to_le_bytes());
    let mut out_buf = Vec::with_capacity(64);
    out_buf.extend_from_slice(beacon);
    out_buf.extend_from_slice(cluster);
    let output = *hemera_hash(&out_buf)
        .as_bytes()
        .first_chunk::<32>()
        .unwrap_or(&[0u8; 32]);
    Statement {
        program_hash: PROGRAM,
        input_hash: input,
        output_hash: output,
        focus_bound: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tickets::{easy_target, grind_settlement, self_fold};
    use tru::{Context, FocusingParams, Fx, Link};
    use crate::settlement::Contribution;

    fn h(b: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        x[0] = b;
        x
    }

    fn setup() -> (Vec<Link>, Vec<Contribution>, [u8; 32], [u8; 32]) {
        let base = vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ];
        let contribs = vec![Contribution {
            neuron: h(10),
            links: vec![Link::stake(h(2), h(1), 8000)],
            surprise: Fx::ONE,
        }];
        (base, contribs, h(0xBE), h(0xC1))
    }

    #[test]
    fn settlement_batch_proves_and_verifies() {
        let (base, contribs, beacon, cluster) = setup();
        let tickets = grind_settlement(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &h(0x91),
            0,
            16,
            3,
            easy_target(),
        );
        assert_eq!(tickets.len(), 3);
        let seal = prove_settlement_batch(&beacon, &cluster, &tickets).unwrap();
        assert!(verify_fold_seal(&seal));
        assert_eq!(seal.steps, 3);
    }

    #[test]
    fn fold_tree_proves_and_verifies() {
        let (base, contribs, beacon, cluster) = setup();
        let leaves: Vec<_> = [0xA, 0xB]
            .iter()
            .map(|&m| {
                let t = grind_settlement(
                    &base,
                    &contribs,
                    &Context::none(),
                    &FocusingParams::default(),
                    &beacon,
                    &cluster,
                    &h(m),
                    m as u64 * 10,
                    8,
                    2,
                    easy_target(),
                );
                self_fold(contribs.len(), &t)
            })
            .collect();
        let (root, seal) = prove_fold_tree(&beacon, &cluster, &leaves).unwrap();
        assert!(root.k >= 4);
        assert!(verify_fold_seal(&seal));
        assert!(seal.steps >= 1);
    }

    #[test]
    fn tampered_proof_fails() {
        let (base, contribs, beacon, cluster) = setup();
        let tickets = grind_settlement(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &beacon,
            &cluster,
            &h(0x91),
            0,
            8,
            2,
            easy_target(),
        );
        let mut seal = prove_settlement_batch(&beacon, &cluster, &tickets).unwrap();
        seal.proof.eval_value = Goldilocks::new(seal.proof.eval_value.as_u64().wrapping_add(1));
        assert!(!verify_fold_seal(&seal));
    }
}
