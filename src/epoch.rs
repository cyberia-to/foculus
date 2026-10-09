// ---
// tags: foculus, rust, rewards, epoch
// crystal-type: source
// crystal-domain: cyber
// ---
//! Live epoch runner: propose → freeze → outer VDF beacon → proven ticket settle.
//!
//! This is the product path that actually runs (not a virtual sketch):
//! ```text
//! propose_claim* → freeze → open_beacon(signal VDFs, T)
//!   → grind tickets → prove each (ticket_proof)
//!   → redraw peer self-accumulators → verify the cluster (cluster)
//!   → SettleReceipt { settlement, beacon_artifact }
//! ```

use tru::{Context, FocusingParams, Link};

use crate::beacon::{
    self, claims_root, open_beacon, verify_beacon, BeaconArtifact, GENESIS_PREV, TEST_OUTER_T,
};
use crate::rewards::{RewardClaim, RewardError, SettleReceipt, TicketPolicy};
use crate::settle_tickets::{ClaimSet, ticket_receipt};
use crate::tickets::ClusterAcc;
use crate::vdf::VdfProof;

/// Epoch lifecycle phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpochPhase {
    /// Accepting claims (propose window).
    Propose,
    /// Claims frozen; beacon not yet opened.
    Frozen,
    /// Outer VDF complete; settle may run.
    BeaconReady,
    /// Settlement complete.
    Settled,
}

/// Live epoch state machine.
#[derive(Clone)]
pub struct EpochRunner {
    pub epoch: u64,
    pub prev_beacon: [u8; 32],
    pub phase: EpochPhase,
    claims: Vec<RewardClaim>,
    pub claims_root: Option<[u8; 32]>,
    pub beacon: Option<BeaconArtifact>,
    pub receipt: Option<SettleReceipt>,
    /// Outer VDF iterations.
    pub outer_t: u64,
    pub budget: u64,
}

impl EpochRunner {
    pub fn new(epoch: u64, prev_beacon: [u8; 32]) -> Self {
        Self {
            epoch,
            prev_beacon,
            phase: EpochPhase::Propose,
            claims: Vec::new(),
            claims_root: None,
            beacon: None,
            receipt: None,
            outer_t: TEST_OUTER_T,
            budget: 1000,
        }
    }

    pub fn genesis(epoch: u64) -> Self {
        Self::new(epoch, GENESIS_PREV)
    }

    pub fn claims(&self) -> &[RewardClaim] {
        &self.claims
    }

    /// Propose window: add a claim. Fails after freeze.
    pub fn propose(&mut self, claim: RewardClaim) -> Result<(), EpochError> {
        if self.phase != EpochPhase::Propose {
            return Err(EpochError::WrongPhase);
        }
        self.claims.push(claim);
        Ok(())
    }

    /// Close propose window — freeze claims_root. Required before beacon.
    pub fn freeze(&mut self) -> Result<[u8; 32], EpochError> {
        if self.phase != EpochPhase::Propose {
            return Err(EpochError::WrongPhase);
        }
        if self.claims.is_empty() {
            return Err(EpochError::EmptyClaims);
        }
        let ids: Vec<[u8; 32]> = self.claims.iter().map(|c| c.id).collect();
        let cr = claims_root(&ids);
        self.claims_root = Some(cr);
        self.phase = EpochPhase::Frozen;
        Ok(cr)
    }

    /// Run outer VDF beacon over finalized signal VDF outputs.
    pub fn open_beacon(&mut self, signal_vdfs: &[VdfProof]) -> Result<&BeaconArtifact, EpochError> {
        if self.phase != EpochPhase::Frozen {
            return Err(EpochError::WrongPhase);
        }
        let cr = self.claims_root.ok_or(EpochError::NotFrozen)?;
        let outputs = beacon::collect_signal_outputs(signal_vdfs);
        let art = open_beacon(
            self.epoch,
            &self.prev_beacon,
            &cr,
            &outputs,
            self.outer_t,
        );
        if !verify_beacon(&art) {
            return Err(EpochError::BeaconInvalid);
        }
        self.beacon = Some(art);
        self.phase = EpochPhase::BeaconReady;
        Ok(self.beacon.as_ref().unwrap())
    }

    /// Quiet beacon (no signal VDFs) — still runs outer VDF_T(prev).
    pub fn open_quiet_beacon(&mut self) -> Result<&BeaconArtifact, EpochError> {
        self.open_beacon(&[])
    }

    /// Settle with proven ticket grind (solo: no peer SelfAccs).
    pub fn settle(
        &mut self,
        base: &[Link],
        ctx: &Context,
        params: &FocusingParams,
        policy: &TicketPolicy,
    ) -> Result<&SettleReceipt, EpochError> {
        self.settle_with_peers(base, ctx, params, policy, &[])
    }

    /// Settle using the **existing** beacon: local tickets plus the tickets
    /// peer SelfAccs name, each redrawn and proven; peers that lie are dropped.
    pub fn settle_with_peers(
        &mut self,
        base: &[Link],
        ctx: &Context,
        params: &FocusingParams,
        policy: &TicketPolicy,
        peer_accs: &[ClusterAcc],
    ) -> Result<&SettleReceipt, EpochError> {
        if self.phase != EpochPhase::BeaconReady {
            return Err(EpochError::WrongPhase);
        }
        if self.budget == 0 {
            return Err(EpochError::BudgetZero);
        }
        let art = self.beacon.as_ref().ok_or(EpochError::NoBeacon)?;
        if !verify_beacon(art) {
            return Err(EpochError::BeaconInvalid);
        }
        let set = ClaimSet { base, claims: &self.claims, ctx, params };
        let mut receipt = ticket_receipt(
            self.epoch,
            &art.beacon,
            &art.claims_root,
            set,
            self.budget,
            policy,
            peer_accs,
        )
        .map_err(|e| match e {
            RewardError::NoTickets => EpochError::NoTickets,
            other => EpochError::Reward(other),
        })?;
        receipt.beacon_artifact = Some(art.clone());
        self.receipt = Some(receipt);
        self.phase = EpochPhase::Settled;
        Ok(self.receipt.as_ref().unwrap())
    }

    /// Full single-node path: propose already filled → freeze → quiet beacon → settle.
    pub fn run_to_settle(
        &mut self,
        base: &[Link],
        ctx: &Context,
        params: &FocusingParams,
        signal_vdfs: &[VdfProof],
        policy: &TicketPolicy,
    ) -> Result<&SettleReceipt, EpochError> {
        if self.phase == EpochPhase::Propose {
            self.freeze()?;
        }
        if self.phase == EpochPhase::Frozen {
            self.open_beacon(signal_vdfs)?;
        }
        self.settle(base, ctx, params, policy)
    }

    /// Advance runner to next epoch after settle (carries prev beacon).
    pub fn next_epoch(&self) -> Result<Self, EpochError> {
        let art = self.beacon.as_ref().ok_or(EpochError::NoBeacon)?;
        if self.phase != EpochPhase::Settled {
            return Err(EpochError::WrongPhase);
        }
        Ok(Self::new(self.epoch.saturating_add(1), art.beacon))
    }
}

/// Structural check of a live-path receipt: beacon VDF, hash binding and a
/// ticket set. Marginal correctness needs the claims and base graph — see
/// [`crate::settle_tickets::verify_settlement`].
pub fn verify_live_receipt(receipt: &SettleReceipt) -> bool {
    let Some(art) = &receipt.beacon_artifact else {
        return false;
    };
    verify_beacon(art)
        && art.beacon == receipt.beacon
        && receipt.settlement.is_some()
        && crate::rewards::verify_receipt(receipt)
}

#[derive(Debug, PartialEq, Eq)]
pub enum EpochError {
    WrongPhase,
    EmptyClaims,
    NotFrozen,
    NoBeacon,
    BeaconInvalid,
    BudgetZero,
    NoTickets,
    Reward(RewardError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewards::claim_from_links;
    use crate::vdf;
    use tru::Link;

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
    fn full_epoch_with_signal_vdfs_and_proofs() {
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
        runner.freeze().unwrap();

        // Simulated finalized signal VDFs (depth-stable S_E)
        let s1 = vdf::evaluate(vdf::challenge_from_hash(&h(0x11)), 16);
        let s2 = vdf::evaluate(vdf::challenge_from_hash(&h(0x22)), 16);
        runner.open_beacon(&[s1, s2]).unwrap();

        let policy = TicketPolicy {
            want: 4,
            max_attempts: 64,
            miner: h(10),
            ..TicketPolicy::default()
        };
        let rec = runner
            .settle(
                &base(),
                &Context::none(),
                &FocusingParams::default(),
                &policy,
            )
            .unwrap();
        assert!(verify_live_receipt(rec));
        assert_eq!(rec.budget, 500);
        assert!(rec.sample_count >= 4);
        assert_eq!(rec.settlement.as_ref().unwrap().tickets.len() as u64, rec.sample_count);
        assert!(rec.beacon_artifact.is_some());
    }

    #[test]
    fn run_to_settle_quiet() {
        let mut runner = EpochRunner::genesis(1);
        runner.budget = 100;
        runner
            .propose(claim_from_links(
                h(0xB2),
                h(11),
                vec![Link::stake(h(3), h(1), 6000)],
                1,
            ))
            .unwrap();
        let policy = TicketPolicy {
            want: 2,
            max_attempts: 32,
            miner: h(11),
            ..TicketPolicy::default()
        };
        let rec = runner
            .run_to_settle(
                &base(),
                &Context::none(),
                &FocusingParams::default(),
                &[],
                &policy,
            )
            .unwrap();
        assert!(verify_live_receipt(rec));
        let next = runner.next_epoch().unwrap();
        assert_eq!(next.epoch, 2);
        assert_eq!(next.prev_beacon, runner.beacon.as_ref().unwrap().beacon);
    }

    #[test]
    fn cannot_propose_after_freeze() {
        let mut runner = EpochRunner::genesis(1);
        runner
            .propose(claim_from_links(h(1), h(10), vec![Link::stake(h(2), h(1), 100)], 1))
            .unwrap();
        runner.freeze().unwrap();
        assert_eq!(
            runner.propose(claim_from_links(h(2), h(11), vec![], 1)),
            Err(EpochError::WrongPhase)
        );
    }
}
