// ---
// tags: foculus, rust, finality, light client
// crystal-type: source
// crystal-domain: cyber
// ---
//! Thin finality evidence — clock A proof objects light clients can verify
//! without running the tri-kernel (cyber/specs/light-money §5, WP4+).
//!
//! Binding covers: signal_id ‖ height ‖ root ‖ nullifier_set_hash ‖ kind.

use bbg::Particle;
use cyber_hemera::hash as hemera_hash;

use tru::Fx;

use crate::finality::{Domain, Finality, finalizes};
use crate::pay_proof::nullifier_set_hash;
use crate::tip::Tip;

const DOMAIN_LOCAL: &[u8] = b"foculus-finality-v1-local";
const DOMAIN_CERT: &[u8] = b"foculus-finality-v1-cert";

/// Kind of finality evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalityKind {
    LocalApplied,
    Certified,
}

/// Portable finality certificate for signal S at tip T.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalityEvidence {
    pub signal_id: Particle,
    pub height: u64,
    pub root: Particle,
    /// Hash of nullifiers in the signal (empty set → dedicated empty hash).
    pub nullifier_hash: Particle,
    pub kind: FinalityKind,
    pub binding: Particle,
}

impl FinalityEvidence {
    pub fn issue_local(signal_id: Particle, tip: &Tip, nullifiers: &[Particle]) -> Self {
        let nullifier_hash = nullifier_set_hash(nullifiers);
        let binding = bind(
            DOMAIN_LOCAL,
            &signal_id,
            tip.height,
            &tip.root,
            &nullifier_hash,
            tip.grade4(),
        );
        Self {
            signal_id,
            height: tip.height,
            root: tip.root,
            nullifier_hash,
            kind: FinalityKind::LocalApplied,
            binding,
        }
    }

    pub fn issue_certified(signal_id: Particle, tip: &Tip, nullifiers: &[Particle]) -> Self {
        let nullifier_hash = nullifier_set_hash(nullifiers);
        let binding = bind(
            DOMAIN_CERT,
            &signal_id,
            tip.height,
            &tip.root,
            &nullifier_hash,
            tip.grade4(),
        );
        Self {
            signal_id,
            height: tip.height,
            root: tip.root,
            nullifier_hash,
            kind: FinalityKind::Certified,
            binding,
        }
    }

    /// Issue certified evidence only when domain-local finalizes() holds
    /// (protocol.md step 6). Light clients still verify the binding; the
    /// issuer attests that φ* + certification gate passed.
    ///
    /// For a full algebraic proof that φ* itself is correct, prove the domain
    /// graph with `zheng::prove_phi_star` and bind `phi_star_hash` off-band
    /// (see zheng/specs/phi-spmv.md).
    pub fn issue_from_domain(
        signal_id: Particle,
        tip: &Tip,
        nullifiers: &[Particle],
        phi_i: Fx,
        domain: &Domain,
        uncert_mass: Fx,
        gap: Fx,
        kappa_d: Fx,
        c: Fx,
        kappa_prime: Fx,
    ) -> Option<Self> {
        match finalizes(phi_i, domain, uncert_mass, gap, kappa_d, c, kappa_prime) {
            Finality::Final => Some(Self::issue_certified(signal_id, tip, nullifiers)),
            Finality::Pending => None,
        }
    }

    pub fn verify(&self, tip: &Tip) -> bool {
        if !tip.grade4() {
            return false;
        }
        if tip.height != self.height || tip.root != self.root {
            return false;
        }
        let domain = match self.kind {
            FinalityKind::LocalApplied => DOMAIN_LOCAL,
            FinalityKind::Certified => DOMAIN_CERT,
        };
        let expect = bind(
            domain,
            &self.signal_id,
            self.height,
            &self.root,
            &self.nullifier_hash,
            true,
        );
        self.binding == expect
    }
}

fn bind(
    domain: &[u8],
    signal_id: &Particle,
    height: u64,
    root: &Particle,
    nullifier_hash: &Particle,
    grade4: bool,
) -> Particle {
    let mut buf = Vec::with_capacity(domain.len() + 32 + 8 + 32 + 32 + 1);
    buf.extend_from_slice(domain);
    buf.extend_from_slice(signal_id);
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(root);
    buf.extend_from_slice(nullifier_hash);
    buf.push(u8::from(grade4));
    let h = hemera_hash(&buf);
    *h.as_bytes().first_chunk::<32>().unwrap_or(&[0u8; 32])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tip::{Tip, join_with_demo_fold};
    use bbg::Checkpoint;

    #[test]
    fn local_evidence_verifies_on_matching_tip() {
        let tip = Tip::from_local(&Checkpoint {
            root: [5u8; 32],
            acc: None,
            height: 4,
        });
        let ev = FinalityEvidence::issue_local([7u8; 32], &tip, &[]);
        assert!(ev.verify(&tip));
    }

    #[test]
    fn evidence_binds_nullifiers() {
        let tip = Tip::from_local(&Checkpoint {
            root: [5u8; 32],
            acc: None,
            height: 4,
        });
        let n1 = [[1u8; 32], [2u8; 32]];
        let ev = FinalityEvidence::issue_local([7u8; 32], &tip, &n1);
        let mut tampered = ev.clone();
        tampered.nullifier_hash = [9u8; 32];
        assert!(!tampered.verify(&tip));
        assert!(ev.verify(&tip));
    }

    #[test]
    fn evidence_rejects_wrong_tip() {
        let tip = Tip::from_local(&Checkpoint {
            root: [5u8; 32],
            acc: None,
            height: 4,
        });
        let ev = FinalityEvidence::issue_local([7u8; 32], &tip, &[]);
        let other = Tip::from_local(&Checkpoint {
            root: [6u8; 32],
            acc: None,
            height: 4,
        });
        assert!(!ev.verify(&other));
    }

    #[test]
    fn certified_evidence_on_fold_tip() {
        let tip = join_with_demo_fold([3u8; 32], 2);
        let ev = FinalityEvidence::issue_certified([9u8; 32], &tip, &[[8u8; 32]]);
        assert!(ev.verify(&tip));
        assert_eq!(ev.kind, FinalityKind::Certified);
    }

    #[test]
    fn issue_from_domain_requires_final() {
        use crate::finality::Domain;
        use tru::Fx;
        let tip = Tip::from_local(&Checkpoint {
            root: [1u8; 32],
            acc: None,
            height: 1,
        });
        // single particle domain: mean = phi, cannot clear μ+κ'σ → Pending
        let p = [9u8; 32];
        let domain = Domain::from_focus(vec![p], vec![Fx::from_int(1)]);
        let none = FinalityEvidence::issue_from_domain(
            p,
            &tip,
            &[],
            Fx::from_int(1),
            &domain,
            Fx::ZERO,
            Fx::from_int(1),
            Fx::from_int(0),
            Fx::from_int(1),
            Fx::from_int(1),
        );
        assert!(none.is_none());
    }
}
