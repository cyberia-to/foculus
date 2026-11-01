// ---
// tags: foculus, rust, node, live, full, partial, light
// crystal-type: source
// crystal-domain: cyber
// ---
//! Unified live node — full / partial / light product stack.
//!
//! Closes the gap between library pieces and a continuous cyber flow:
//! ```text
//! ingest signals → tip advance → epoch propose/freeze/beacon
//!   → grind + prove Δφ⁺ tickets → (radio) SelfAcc → redraw + settle
//!   → EpochCertificate
//! ```
//!
//! Modes ([node-modes](cyber/specs/node-modes.md)):
//! - **Full**: holds signal history, runs settle, issues epoch certs, serves tip
//! - **Partial**: local neuron claims + money-facing settle apply; embeds tip
//! - **Light**: tip join/advance only; verifies epoch certs + openings; no grind

use std::collections::{BTreeMap, BTreeSet};

use bbg::Checkpoint;
use tru::{Context, FocusingParams, Link};

use crate::beacon::TEST_OUTER_T;
use crate::epoch::{EpochPhase, EpochRunner};
use crate::epoch_cert::{
    issue_epoch_cert, verify_epoch_cert, EpochCertificate, SettleVerifyInputs,
};
use crate::rewards::{
    claim_from_links, share_of, verify_receipt, RewardClaim, TicketPolicy,
};
use crate::tickets::{easy_target, grind_settlement, self_fold, ClusterAcc};
use crate::tip::{Tip, TipProver, TipStep};
use crate::vdf::{self, VdfProof};

/// Participation mode. Partial retains the local graph/settlement duties of
/// the former second variant. This enum has no persisted or wire codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeMode {
    Full,
    Partial,
    Light,
}

/// Ingested signal for live graph / propose window.
#[derive(Clone)]
pub struct LiveSignal {
    pub id: [u8; 32],
    pub neuron: [u8; 32],
    pub links: Vec<Link>,
    pub valence: i8,
    /// Per-signal VDF for beacon S_E.
    pub vdf: VdfProof,
}

/// Unified live node state machine.
pub struct LiveNode {
    pub mode: NodeMode,
    pub neuron: [u8; 32],
    tip: Tip,
    tip_prover: Option<TipProver>,
    /// Full/partial: signal log by id.
    signals: BTreeMap<[u8; 32], LiveSignal>,
    /// Base graph for focusing (full/partial).
    pub reward_base: Vec<Link>,
    epoch: EpochRunner,
    /// Last issued certificate.
    pub last_cert: Option<EpochCertificate>,
    /// Peer SelfAccs received (full/partial settler).
    peer_accs: Vec<ClusterAcc>,
    pub budget: u64,
    pub settle_depth: u64,
    /// Clock-B pending mints for this neuron: (amount, reason, mint_height).
    pending_rewards: Vec<(u64, [u8; 32], u64)>,
    /// Receipt hashes already credited (idempotent accept_epoch_cert).
    credited_receipts: BTreeSet<[u8; 32]>,
    /// Spendable reward balance (after maturity) for diagnostics.
    pub matured_reward: u64,
}

impl LiveNode {
    pub fn new(mode: NodeMode, neuron: [u8; 32]) -> Self {
        let (tip, tip_prover) = match mode {
            NodeMode::Light => (Tip::untrusted(), None),
            NodeMode::Full | NodeMode::Partial => {
                let mut prover = TipProver::new();
                let _ = prover.push_height(0, [0u8; 32]);
                let tip = prover.seal_tip().unwrap_or_else(|_| Tip::from_local(&Checkpoint {
                    root: [0u8; 32],
                    acc: None,
                    height: 0,
                }));
                (tip, Some(prover))
            }
        };
        Self {
            mode,
            neuron,
            tip,
            tip_prover,
            signals: BTreeMap::new(),
            reward_base: Vec::new(),
            epoch: EpochRunner::genesis(1),
            last_cert: None,
            peer_accs: Vec::new(),
            budget: 1000,
            settle_depth: 2,
            pending_rewards: Vec::new(),
            credited_receipts: BTreeSet::new(),
            matured_reward: 0,
        }
    }

    pub fn tip(&self) -> &Tip {
        &self.tip
    }

    pub fn grade4(&self) -> bool {
        self.tip.grade4()
    }

    pub fn epoch_phase(&self) -> EpochPhase {
        self.epoch.phase
    }

    pub fn claims(&self) -> &[RewardClaim] {
        self.epoch.claims()
    }

    /// Light: join from a full/partial tip checkpoint.
    pub fn light_join(&mut self, tip: Tip) -> Result<(), LiveError> {
        if self.mode != NodeMode::Light {
            return Err(LiveError::WrongMode);
        }
        if !tip.grade4() {
            return Err(LiveError::TipUntrusted);
        }
        self.tip = tip;
        Ok(())
    }

    /// Light: extend the chain tip by one step whose new head is trusted (clock C).
    pub fn light_advance(&mut self, step: TipStep, trusted_head: &[u8; 32]) -> Result<(), LiveError> {
        if self.mode != NodeMode::Light {
            return Err(LiveError::WrongMode);
        }
        self.tip
            .advance_chain(step, trusted_head)
            .map_err(|_| LiveError::TipAdvanceFailed)?;
        Ok(())
    }

    /// Full/partial: the step chain a light client joins with, and its head.
    pub fn tip_chain(&self) -> Option<(&crate::tip::TipChain, [u8; 32])> {
        self.tip_prover.as_ref().map(|p| (p.chain(), p.head()))
    }

    /// Full/partial: ingest a signal into history and propose-window claim.
    pub fn ingest_signal(&mut self, sig: LiveSignal) -> Result<(), LiveError> {
        if matches!(self.mode, NodeMode::Light) {
            return Err(LiveError::WrongMode);
        }
        if self.signals.contains_key(&sig.id) {
            return Ok(()); // idempotent
        }
        // Propose claim while epoch is open
        if self.epoch.phase == EpochPhase::Propose {
            let claim = claim_from_links(sig.id, sig.neuron, sig.links.clone(), sig.valence);
            self.epoch
                .propose(claim)
                .map_err(|_| LiveError::EpochError)?;
        }
        self.signals.insert(sig.id, sig);
        // Advance tip one height
        self.bump_tip()?;
        Ok(())
    }

    /// Convenience: local neuron creates a reward link signal.
    pub fn link(
        &mut self,
        from: [u8; 32],
        to: [u8; 32],
        amount: u128,
        valence: i8,
    ) -> Result<[u8; 32], LiveError> {
        let mut id = [0u8; 32];
        id[0..8].copy_from_slice(&(self.signals.len() as u64 + 1).to_le_bytes());
        id[8] = self.neuron[0];
        let challenge = vdf::challenge_from_hash(&id);
        let vdf_p = vdf::evaluate(challenge, 16);
        let mut link = Link::stake(from, to, amount);
        link.neuron = self.neuron;
        link.valence = valence;
        self.ingest_signal(LiveSignal {
            id,
            neuron: self.neuron,
            links: vec![link],
            valence,
            vdf: vdf_p,
        })?;
        Ok(id)
    }

    /// Absorb peer SelfAcc (full/partial settler).
    pub fn absorb_peer_acc(&mut self, acc: ClusterAcc) {
        if !matches!(self.mode, NodeMode::Full | NodeMode::Partial) {
            return;
        }
        self.peer_accs.push(acc);
    }

    /// Close propose, open beacon from signal VDFs, settle, issue epoch cert.
    ///
    /// This is the continuous consensus→rewards step for full/partial.
    pub fn close_and_settle_epoch(&mut self) -> Result<EpochCertificate, LiveError> {
        if matches!(self.mode, NodeMode::Light) {
            return Err(LiveError::WrongMode);
        }
        self.epoch.budget = self.budget;
        self.epoch.outer_t = TEST_OUTER_T;

        if self.epoch.phase == EpochPhase::Propose {
            if self.epoch.claims().is_empty() {
                return Err(LiveError::EmptyClaims);
            }
            self.epoch.freeze().map_err(|_| LiveError::EpochError)?;
        }
        if self.epoch.phase == EpochPhase::Frozen {
            let vdfs: Vec<VdfProof> = self.signals.values().map(|s| s.vdf.clone()).collect();
            self.epoch
                .open_beacon(&vdfs)
                .map_err(|_| LiveError::EpochError)?;
        }

        let ctx = Context::none();
        let params = FocusingParams::default();
        let policy = TicketPolicy {
            want: 4,
            max_attempts: 64,
            miner: self.neuron,
            settle_target: easy_target(),
            ..TicketPolicy::default()
        };

        let peers = self.peer_accs.clone();
        let rec = if peers.is_empty() {
            self.epoch
                .settle(&self.reward_base, &ctx, &params, &policy)
                .map_err(|_| LiveError::SettleFailed)?
                .clone()
        } else {
            self.epoch
                .settle_with_peers(&self.reward_base, &ctx, &params, &policy, &peers)
                .map_err(|_| LiveError::SettleFailed)?
                .clone()
        };

        if !verify_receipt(&rec) {
            return Err(LiveError::SettleFailed);
        }
        // The receipt carries every ticket proof; verify_epoch_cert below
        // re-verifies them from the public claims and base graph.
        let cr = rec.claims_root;
        self.credit_receipt(&rec, self.tip.height);

        let art = self.epoch.beacon.clone().ok_or(LiveError::EpochError)?;
        let cert = issue_epoch_cert(
            self.epoch.epoch,
            &self.tip,
            art,
            cr,
            Some(rec),
            None,
        );
        let inputs = SettleVerifyInputs {
            base: &self.reward_base,
            claims: self.epoch.claims(),
            ctx: &ctx,
            params: &params,
            target: easy_target(),
        };
        if !verify_epoch_cert(&cert, Some(&inputs)) {
            return Err(LiveError::CertInvalid);
        }
        self.last_cert = Some(cert.clone());
        self.peer_accs.clear();

        if let Ok(next) = self.epoch.next_epoch() {
            self.epoch = next;
            self.epoch.budget = self.budget;
        }
        self.mature_rewards();
        Ok(cert)
    }

    fn credit_receipt(&mut self, rec: &crate::rewards::SettleReceipt, mint_height: u64) {
        if !self.credited_receipts.insert(rec.receipt_hash) {
            return; // already applied
        }
        let amt = share_of(rec, &self.neuron);
        if amt > 0 {
            self.pending_rewards
                .push((amt, rec.receipt_hash, mint_height));
        }
    }

    /// Light/full/partial: verify a peer-issued epoch certificate.
    pub fn accept_epoch_cert(
        &mut self,
        cert: EpochCertificate,
        claims: Option<&[RewardClaim]>,
    ) -> Result<(), LiveError> {
        let ctx = Context::none();
        let params = FocusingParams::default();
        let ok = if let Some(c) = claims {
            let inputs = SettleVerifyInputs {
                base: &self.reward_base,
                claims: c,
                ctx: &ctx,
                params: &params,
                target: easy_target(),
            };
            verify_epoch_cert(&cert, Some(&inputs))
        } else {
            verify_epoch_cert(&cert, None)
        };
        if !ok {
            return Err(LiveError::CertInvalid);
        }
        // A certificate does not prove its tip root: a light node follows the
        // height but keeps no trust in a root its own chain has not reached.
        if self.mode == NodeMode::Light
            && cert.tip_height > self.tip.height
        {
            self.tip = Tip::join_checkpoint(&Checkpoint {
                root: cert.tip_root,
                acc: None,
                height: cert.tip_height,
            });
        }
        if let Some(rec) = &cert.settle {
            self.credit_receipt(rec, cert.tip_height);
        }
        self.last_cert = Some(cert);
        self.mature_rewards();
        Ok(())
    }

    /// Publish local SelfAcc for multi-miner (full/partial). Requires BeaconReady.
    pub fn mine_self_acc(&self) -> Result<ClusterAcc, LiveError> {
        if matches!(self.mode, NodeMode::Light) {
            return Err(LiveError::WrongMode);
        }
        if self.epoch.phase != EpochPhase::BeaconReady {
            return Err(LiveError::BeaconNotReady);
        }
        let art = self
            .epoch
            .beacon
            .as_ref()
            .ok_or(LiveError::BeaconNotReady)?;
        let claims = self.epoch.claims();
        if claims.is_empty() {
            return Err(LiveError::EmptyClaims);
        }
        let contribs = crate::rewards::contributions_with_rho(claims);
        let tickets = grind_settlement(
            &self.reward_base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &art.beacon,
            &art.claims_root,
            &self.neuron,
            0,
            64,
            3,
            easy_target(),
        );
        if tickets.is_empty() {
            return Err(LiveError::SettleFailed);
        }
        Ok(self_fold(contribs.len(), &tickets))
    }

    /// After freeze, open beacon without settle (so peers can mine).
    pub fn freeze_and_beacon(&mut self) -> Result<[u8; 32], LiveError> {
        if matches!(self.mode, NodeMode::Light) {
            return Err(LiveError::WrongMode);
        }
        if self.epoch.phase == EpochPhase::Propose {
            self.epoch.freeze().map_err(|_| LiveError::EpochError)?;
        }
        if self.epoch.phase == EpochPhase::Frozen {
            let vdfs: Vec<_> = self.signals.values().map(|s| s.vdf.clone()).collect();
            self.epoch
                .open_beacon(&vdfs)
                .map_err(|_| LiveError::EpochError)?;
        }
        Ok(self.epoch.claims_root.unwrap_or([0u8; 32]))
    }

    fn bump_tip(&mut self) -> Result<(), LiveError> {
        let h = self.tip.height.saturating_add(1);
        let root = {
            let mut r = [0u8; 32];
            r[0..8].copy_from_slice(&h.to_le_bytes());
            r[8] = self.signals.len() as u8;
            r
        };
        if let Some(prover) = self.tip_prover.as_mut() {
            prover
                .push_height(h, root)
                .map_err(|_| LiveError::TipAdvanceFailed)?;
            self.tip = prover.seal_tip().map_err(|_| LiveError::TipAdvanceFailed)?;
        } else {
            self.tip = Tip::from_local(&Checkpoint {
                root,
                acc: None,
                height: h,
            });
        }
        self.mature_rewards();
        Ok(())
    }

    fn mature_rewards(&mut self) {
        let h = self.tip.height;
        let depth = self.settle_depth;
        let mut keep = Vec::new();
        for (amt, reason, mint_h) in self.pending_rewards.drain(..) {
            if h >= mint_h.saturating_add(depth) {
                self.matured_reward = self.matured_reward.saturating_add(amt);
                let _ = reason;
            } else {
                keep.push((amt, reason, mint_h));
            }
        }
        self.pending_rewards = keep;
    }

    /// Export tip for light clients (full/partial).
    pub fn export_tip(&self) -> Tip {
        self.tip.clone()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LiveError {
    WrongMode,
    TipUntrusted,
    TipAdvanceFailed,
    EpochError,
    EmptyClaims,
    BeaconNotReady,
    SettleFailed,
    CertInvalid,
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
