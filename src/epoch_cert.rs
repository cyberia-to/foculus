// ---
// tags: foculus, rust, consensus, epoch, proof
// crystal-type: source
// crystal-domain: cyber
// ---
//! Epoch certificate — what consensus finalized for one epoch and what it
//! settled.
//!
//! Binds:
//! - tip (height, root) and its trust grade
//! - claims_root + outer VDF beacon
//! - optional φ* claim: graph commitment, φ₀ hash, φ* hash, iterations
//! - optional settle receipt, whose ticket proofs ride inside it
//!
//! [`verify_epoch_cert`] fails closed: a certificate that settles needs the
//! settle inputs, one that binds φ* needs the graphs. Nothing a certificate
//! binds is accepted unchecked.

use cyber_hemera::hash as hemera_hash;
use nebu::Goldilocks;

use crate::beacon::{verify_beacon, BeaconArtifact};
use crate::phi::{verify_phi_claim, PhiBinding, PhiGraph};
use crate::rewards::SettleReceipt;
use crate::tip::{Tip, TipTrust};

pub use crate::settle_tickets::{verify_settlement, ReceiptError, SettleVerifyInputs};

/// Full epoch certificate for network consensus + optional settle.
#[derive(Clone)]
pub struct EpochCertificate {
    pub epoch: u64,
    pub tip_height: u64,
    pub tip_root: [u8; 32],
    pub tip_trust: TipTrust,
    pub claims_root: [u8; 32],
    pub beacon: BeaconArtifact,
    /// φ* claim (if a domain computation was issued).
    pub phi: Option<PhiBinding>,
    /// Optional settle output, carrying its ticket proofs.
    pub settle: Option<SettleReceipt>,
    /// Certificate hash binding all fields above.
    pub cert_hash: [u8; 32],
}

/// The graphs and φ₀ a φ* binding is checked against.
pub struct PhiVerifyInputs<'a> {
    pub graph: PhiGraph<'a>,
    pub phi0: &'a [Goldilocks],
}

/// Settlement digest: every accepted ticket's identity and commitment.
fn settlement_digest(rec: &SettleReceipt) -> [u8; 32] {
    let mut buf = Vec::new();
    if let Some(proof) = &rec.settlement {
        buf.extend_from_slice(&proof.target.to_le_bytes());
        for t in &proof.tickets {
            buf.extend_from_slice(&t.ticket.miner);
            buf.extend_from_slice(&t.ticket.nonce.to_le_bytes());
            buf.extend_from_slice(&t.ticket.commitment);
            buf.extend_from_slice(t.proof.statement());
        }
    }
    *hemera_hash(&buf).as_bytes()
}

/// Build cert hash.
pub fn cert_hash(c: &EpochCertificate) -> [u8; 32] {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(b"foculus-epoch-cert-v3");
    buf.extend_from_slice(&c.epoch.to_le_bytes());
    buf.extend_from_slice(&c.tip_height.to_le_bytes());
    buf.extend_from_slice(&c.tip_root);
    buf.extend_from_slice(&(c.tip_trust as u8).to_le_bytes());
    buf.extend_from_slice(&c.claims_root);
    buf.extend_from_slice(&c.beacon.beacon);
    match &c.settle {
        Some(s) => {
            buf.push(1);
            buf.extend_from_slice(&s.receipt_hash);
            buf.extend_from_slice(&settlement_digest(s));
        }
        None => buf.push(0),
    }
    match &c.phi {
        Some(p) => {
            buf.push(1);
            buf.extend_from_slice(&p.graph_commit);
            buf.extend_from_slice(&p.phi0_hash);
            buf.extend_from_slice(&p.phi_star_hash);
            buf.extend_from_slice(&p.iterations.to_le_bytes());
        }
        None => buf.push(0),
    }
    *hemera_hash(&buf).as_bytes()
}

/// Issue certificate from tip + beacon + optional settle/φ*.
pub fn issue_epoch_cert(
    epoch: u64,
    tip: &Tip,
    beacon: BeaconArtifact,
    claims_root: [u8; 32],
    settle: Option<SettleReceipt>,
    phi: Option<PhiBinding>,
) -> EpochCertificate {
    let mut c = EpochCertificate {
        epoch,
        tip_height: tip.height,
        tip_root: tip.root,
        tip_trust: tip.trust,
        claims_root,
        beacon,
        phi,
        settle,
        cert_hash: [0u8; 32],
    };
    c.cert_hash = cert_hash(&c);
    c
}

/// Verify a certificate that binds no φ*. See [`verify_epoch_cert_with_phi`].
pub fn verify_epoch_cert(
    cert: &EpochCertificate,
    settle_inputs: Option<&SettleVerifyInputs<'_>>,
) -> bool {
    verify_epoch_cert_with_phi(cert, settle_inputs, None)
}

/// Verify an epoch certificate: hash, tip grade, beacon VDF, claims root;
/// the settle receipt in full (every ticket proof, shares, receipt hash)
/// when one is bound; φ* by recomputation over the bound graphs when one is
/// bound. A bound part without its inputs fails.
pub fn verify_epoch_cert_with_phi(
    cert: &EpochCertificate,
    settle_inputs: Option<&SettleVerifyInputs<'_>>,
    phi_inputs: Option<&PhiVerifyInputs<'_>>,
) -> bool {
    if cert.cert_hash != cert_hash(cert) {
        return false;
    }
    if !matches!(cert.tip_trust, TipTrust::LocalApplied | TipTrust::ChainVerified) {
        return false;
    }
    if !verify_beacon(&cert.beacon) {
        return false;
    }
    if cert.beacon.claims_root != cert.claims_root || cert.beacon.epoch != cert.epoch {
        return false;
    }
    if let Some(rec) = &cert.settle {
        let Some(inputs) = settle_inputs else {
            return false;
        };
        if rec.epoch != cert.epoch
            || rec.beacon != cert.beacon.beacon
            || rec.claims_root != cert.claims_root
            || verify_settlement(rec, inputs).is_err()
        {
            return false;
        }
    }
    match (&cert.phi, phi_inputs) {
        (None, _) => true,
        (Some(binding), Some(inp)) => verify_phi_claim(binding, &inp.graph, inp.phi0),
        (Some(_), None) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{open_beacon, GENESIS_PREV, TEST_OUTER_T};
    use crate::epoch::EpochRunner;
    use crate::rewards::{claim_from_links, TicketPolicy};
    use crate::tip::Tip;
    use crate::tickets::easy_target;
    use bbg::Checkpoint;
    use tru::{Context, FocusingParams, Link};

    fn h(b: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        x[0] = b;
        x
    }

    #[test]
    fn epoch_cert_from_runner_verifies() {
        let mut runner = EpochRunner::genesis(1);
        runner.budget = 500;
        runner.outer_t = TEST_OUTER_T;
        runner
            .propose(claim_from_links(
                h(0xA1),
                h(10),
                vec![Link::stake(h(2), h(1), 8000)],
                1,
            ))
            .unwrap();
        let cr = runner.freeze().unwrap();
        runner.open_quiet_beacon().unwrap();
        let base = vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ];
        let policy = TicketPolicy {
            want: 2,
            max_attempts: 32,
            miner: h(10),
            settle_target: easy_target(),
            ..TicketPolicy::default()
        };
        let rec = runner
            .settle(
                &base,
                &Context::none(),
                &FocusingParams::default(),
                &policy,
            )
            .unwrap()
            .clone();
        let tip = Tip::from_local(&Checkpoint {
            root: h(0xB1),
            acc: None,
            height: 3,
        });
        let art = runner.beacon.clone().unwrap();
        let cert = issue_epoch_cert(
            1,
            &tip,
            art,
            cr,
            Some(rec.clone()),
            None,
        );
        let cert2 = cert.clone();
        assert_eq!(cert.cert_hash, cert2.cert_hash);
        let inputs = SettleVerifyInputs {
            base: &base,
            claims: runner.claims(),
            ctx: &Context::none(),
            params: &FocusingParams::default(),
            target: easy_target(),
        };
        assert!(verify_epoch_cert(&cert, Some(&inputs)));
        // a settling certificate without its inputs fails closed
        assert!(!verify_epoch_cert(&cert2, None));
        // another base graph gives other marginals: rejected
        let other_base = vec![Link::stake(h(1), h(3), 50)];
        let wrong = SettleVerifyInputs {
            base: &other_base,
            ..inputs
        };
        assert!(!verify_epoch_cert(&cert, Some(&wrong)));
    }

    #[test]
    fn bad_beacon_fails() {
        let tip = Tip::from_local(&Checkpoint {
            root: h(1),
            acc: None,
            height: 1,
        });
        let mut art = open_beacon(1, &GENESIS_PREV, &h(2), &[], TEST_OUTER_T);
        art.beacon[0] ^= 1;
        let cert = issue_epoch_cert(1, &tip, art, h(2), None, None);
        // cert_hash was computed with bad beacon already — verify_beacon fails
        assert!(!verify_beacon(&cert.beacon) || !verify_epoch_cert(&cert, None));
    }
}
