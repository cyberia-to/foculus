// ---
// tags: foculus, rust, node, live, full, cell, light
// crystal-type: source
// crystal-domain: cyber
// ---
//! Unified live node — full / cell / light product stack.
//!
//! Closes the gap between library pieces and a continuous cyber flow:
//! ```text
//! ingest signals → tip advance → epoch propose/freeze/beacon
//!   → grind + certify Δφ⁺ tickets → (radio) SelfAcc → settle
//!   → EpochCertificate
//! ```
//!
//! Modes ([node-modes](cyber/specs/node-modes.md)):
//! - **Full**: holds signal history, runs settle, issues epoch certs, serves tip
//! - **Cell**: local neuron claims + money-facing settle apply; embeds tip
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
use crate::tip::{Tip, TipProver, TipTrust};
use crate::vdf::{self, VdfProof};

/// Participation mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeMode {
    Full,
    Cell,
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
    /// Full/cell: signal log by id.
    signals: BTreeMap<[u8; 32], LiveSignal>,
    /// Base graph for focusing (full/cell).
    pub reward_base: Vec<Link>,
    epoch: EpochRunner,
    /// Last issued certificate.
    pub last_cert: Option<EpochCertificate>,
    /// Peer SelfAccs received (full/cell settler).
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
            NodeMode::Full | NodeMode::Cell => {
                let mut prover = TipProver::new();
                let _ = prover.fold_height(0, [0u8; 32]);
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

    /// Light: join from a full/cell tip checkpoint.
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

    /// Light: advance fold tip (clock C).
    pub fn light_advance(&mut self, height: u64, root: [u8; 32]) -> Result<(), LiveError> {
        if self.mode != NodeMode::Light {
            return Err(LiveError::WrongMode);
        }
        self.tip
            .advance_fold(height, root)
            .map_err(|_| LiveError::TipAdvanceFailed)?;
        Ok(())
    }

    /// Full/cell: ingest a signal into history and propose-window claim.
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

    /// Absorb peer SelfAcc (full/cell settler).
    pub fn absorb_peer_acc(&mut self, acc: ClusterAcc) {
        if !matches!(self.mode, NodeMode::Full | NodeMode::Cell) {
            return;
        }
        self.peer_accs.push(acc);
    }

    /// Close propose, open beacon from signal VDFs, settle, issue epoch cert.
    ///
    /// This is the continuous consensus→rewards step for full/cell.
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
        // Receipt already carries ticket_seal + fold_seal (HyperNova).
        // Marginal replay is enforced when callers use marginal_cert on tickets.
        let cr = rec.claims_root;
        let batch_seal = rec.ticket_seal.clone();
        self.credit_receipt(&rec, self.tip.height);

        let art = self.epoch.beacon.clone().ok_or(LiveError::EpochError)?;
        let cert = issue_epoch_cert(
            self.epoch.epoch,
            &self.tip,
            art,
            cr,
            Some(rec),
            batch_seal,
            None,
        );
        let inputs = SettleVerifyInputs {
            base: &self.reward_base,
            claims: self.epoch.claims(),
            ctx: &ctx,
            params: &params,
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

    /// Light/full/cell: verify a peer-issued epoch certificate.
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
            };
            verify_epoch_cert(&cert, Some(&inputs))
        } else {
            verify_epoch_cert(&cert, None)
        };
        if !ok {
            return Err(LiveError::CertInvalid);
        }
        // Light advances tip to cert tip if fold mode
        if self.mode == NodeMode::Light {
            if cert.tip_trust == TipTrust::FoldDecided || cert.tip_height >= self.tip.height {
                // Import tip height/root as local applied for test path when we only have cert
                self.tip = Tip::from_local(&Checkpoint {
                    root: cert.tip_root,
                    acc: None,
                    height: cert.tip_height,
                });
            }
        }
        if let Some(rec) = &cert.settle {
            self.credit_receipt(rec, cert.tip_height);
        }
        self.last_cert = Some(cert);
        self.mature_rewards();
        Ok(())
    }

    /// Publish local SelfAcc for multi-miner (full/cell). Requires BeaconReady.
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
                .fold_height(h, root)
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

    /// Export tip for light clients (full/cell).
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
mod tests {
    use super::*;

    fn h(b: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        x[0] = b;
        x
    }

    fn base() -> Vec<Link> {
        vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ]
    }

    #[test]
    fn full_node_link_settle_and_mature() {
        let mut full = LiveNode::new(NodeMode::Full, h(10));
        full.reward_base = base();
        full.budget = 500;
        full.settle_depth = 1;
        full.link(h(2), h(1), 8000, 1).unwrap();
        assert!(full.grade4());
        let cert = full.close_and_settle_epoch().unwrap();
        let rec = cert.settle.as_ref().expect("settle");
        assert!(verify_receipt(rec));
        assert_eq!(share_of(rec, &h(10)), 500);
        assert!(full.pending_rewards.iter().any(|(a, _, _)| *a == 500));
        full.bump_tip().unwrap(); // mature depth 1
        assert_eq!(full.matured_reward, 500);
        // cert clone stays verifiable
        assert!(verify_epoch_cert(&cert.clone(), None));
    }

    #[test]
    fn multi_node_swarm_leaderless_settle() {
        let mut alice = LiveNode::new(NodeMode::Full, h(10));
        let mut bob = LiveNode::new(NodeMode::Full, h(11));
        let mut carol = LiveNode::new(NodeMode::Light, h(12));
        alice.reward_base = base();
        bob.reward_base = base();
        alice.budget = 1000;
        bob.budget = 1000;
        alice.settle_depth = 0;
        bob.settle_depth = 0;
        carol.settle_depth = 0;

        alice.link(h(2), h(1), 8000, 1).unwrap();
        bob.link(h(3), h(1), 6000, 1).unwrap();

        // Cross-gossip signals
        for s in bob.signals.values().cloned().collect::<Vec<_>>() {
            alice.ingest_signal(s).ok();
        }
        for s in alice.signals.values().cloned().collect::<Vec<_>>() {
            bob.ingest_signal(s).ok();
        }

        // Shared claim set on alice's epoch runner
        alice.epoch = EpochRunner::genesis(1);
        alice.epoch.budget = 1000;
        for s in alice.signals.values() {
            alice
                .epoch
                .propose(claim_from_links(s.id, s.neuron, s.links.clone(), s.valence))
                .unwrap();
        }
        alice.freeze_and_beacon().unwrap();

        // Bob mirrors claim set + beacon (network agreement)
        bob.epoch = EpochRunner::genesis(1);
        bob.epoch.budget = 1000;
        for s in alice.signals.values() {
            bob.epoch
                .propose(claim_from_links(s.id, s.neuron, s.links.clone(), s.valence))
                .unwrap();
        }
        bob.epoch.freeze().unwrap();
        bob.epoch.beacon = alice.epoch.beacon.clone();
        bob.epoch.phase = EpochPhase::BeaconReady;
        bob.epoch.claims_root = alice.epoch.claims_root;

        let acc_b = bob.mine_self_acc().unwrap();
        let acc_a = alice.mine_self_acc().unwrap();
        alice.absorb_peer_acc(acc_b);
        alice.absorb_peer_acc(acc_a);

        let cert = alice.close_and_settle_epoch().unwrap();
        let rec = cert.settle.as_ref().unwrap();
        assert_eq!(rec.shares.iter().map(|s| s.amount).sum::<u64>(), 1000);
        assert!(rec.sample_count >= 2);

        // Light accepts cert (idempotent)
        let claims: Vec<_> = alice
            .signals
            .values()
            .map(|s| claim_from_links(s.id, s.neuron, s.links.clone(), s.valence))
            .collect();
        carol.reward_base = base();
        carol.accept_epoch_cert(cert.clone(), Some(&claims)).unwrap();
        carol.accept_epoch_cert(cert, Some(&claims)).unwrap(); // no double credit
        assert!(carol.tip().height > 0);
        assert!(carol.last_cert.is_some());
    }

    #[test]
    fn cell_mode_same_as_full_for_local_settle() {
        let mut cell = LiveNode::new(NodeMode::Cell, h(10));
        cell.reward_base = base();
        cell.budget = 200;
        cell.settle_depth = 0;
        cell.link(h(2), h(1), 5000, 1).unwrap();
        let cert = cell.close_and_settle_epoch().unwrap();
        assert!(verify_epoch_cert(&cert, None));
        assert_eq!(cell.matured_reward, 200);
    }

    #[test]
    fn light_cannot_ingest() {
        let mut light = LiveNode::new(NodeMode::Light, h(1));
        assert_eq!(light.link(h(2), h(1), 1, 1), Err(LiveError::WrongMode));
    }

    #[test]
    fn mine_requires_beacon() {
        let mut n = LiveNode::new(NodeMode::Full, h(10));
        n.reward_base = base();
        n.link(h(2), h(1), 100, 1).unwrap();
        assert!(matches!(n.mine_self_acc(), Err(LiveError::BeaconNotReady)));
    }
}
