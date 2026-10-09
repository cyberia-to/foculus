// ---
// tags: foculus, rust, tip, light client
// crystal-type: source
// crystal-domain: cyber
// ---
//! Tip service — clock C (history trust) for full / partial / light modes.
//!
//! Normative: cyber/specs/light-money.md, money-loop.md.
//!
//! A light client's tip is a chain of steps `(height, root, leaves)` linked
//! by hemera: `head_i = H(head_{i−1} ‖ height_i ‖ root_i ‖ leaves_i)`, heights
//! strictly increasing. [`Tip::join_chain`] recomputes the chain — linear,
//! one hash per step — and trusts the tip only if the head equals a head the
//! caller already trusts (finality evidence, an epoch certificate, a peer
//! set it has decided to follow).
//!
//! What this establishes: the root is the last step of the chain the
//! trusted head commits to. What it does not: that each root is a valid
//! state transition from the one before. That needs a step relation over
//! bbg transitions and accumulation of its proofs — phase 3 of the proof
//! repair. The legacy fold accumulator this replaces bound neither: its
//! statement was derived from the checkpoint it was meant to check.

use bbg::{Checkpoint, Particle};

use crate::proof::digest;

const CHAIN_DOMAIN: &[u8] = b"foculus-tip-chain-v1";

/// How the tip root was established (certainty grade 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TipTrust {
    /// No trusted tip — money openings fail closed.
    Untrusted = 0,
    /// Full node or partial node: history applied locally.
    LocalApplied = 1,
    /// Light client: step chain recomputed up to a trusted head.
    ChainVerified = 2,
}

/// One step of the tip chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TipStep {
    pub height: u64,
    pub root: Particle,
    /// Commitment to the root's leaves (dimension roots); zero if unused.
    pub leaves: Particle,
}

/// The steps from genesis to a tip.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TipChain {
    pub steps: Vec<TipStep>,
}

fn link(prev: &Particle, step: &TipStep) -> Particle {
    digest(
        CHAIN_DOMAIN,
        &[prev, &step.height.to_le_bytes(), &step.root, &step.leaves],
    )
}

impl TipChain {
    /// Recompute the head; `None` if empty or heights do not strictly increase.
    pub fn verify(&self) -> Option<Particle> {
        let mut head = [0u8; 32];
        let mut last: Option<u64> = None;
        for step in &self.steps {
            if last.is_some_and(|h| step.height <= h) {
                return None;
            }
            last = Some(step.height);
            head = link(&head, step);
        }
        last.map(|_| head)
    }
}

/// Trusted tip object (cyber/specs/light-money §2).
#[derive(Clone, Debug)]
pub struct Tip {
    pub height: u64,
    pub root: Particle,
    pub trust: TipTrust,
    /// Chain head the tip was verified under (light mode).
    pub head: Option<Particle>,
}

impl Tip {
    pub fn grade4(&self) -> bool {
        matches!(self.trust, TipTrust::LocalApplied | TipTrust::ChainVerified)
    }

    pub fn from_local(checkpoint: &Checkpoint) -> Self {
        Self {
            height: checkpoint.height,
            root: checkpoint.root,
            trust: TipTrust::LocalApplied,
            head: None,
        }
    }

    pub fn untrusted() -> Self {
        Self {
            height: 0,
            root: [0u8; 32],
            trust: TipTrust::Untrusted,
            head: None,
        }
    }

    /// Light join from a bbg checkpoint. Its accumulator is the legacy
    /// fold object (unsound, feature `legacy`), which proves nothing
    /// about the root, so the tip stays untrusted; light clients join
    /// with [`Tip::join_chain`].
    pub fn join_checkpoint(checkpoint: &Checkpoint) -> Self {
        Self {
            height: checkpoint.height,
            root: checkpoint.root,
            trust: TipTrust::Untrusted,
            head: None,
        }
    }

    /// Light join: recompute the chain, trust its last step iff the head
    /// equals `trusted_head`.
    pub fn join_chain(chain: &TipChain, trusted_head: &Particle) -> Self {
        let (Some(head), Some(last)) = (chain.verify(), chain.steps.last()) else {
            return Self::untrusted();
        };
        let trust = if head == *trusted_head {
            TipTrust::ChainVerified
        } else {
            TipTrust::Untrusted
        };
        Self {
            height: last.height,
            root: last.root,
            trust,
            head: Some(head),
        }
    }

    pub fn advance_local(&mut self, checkpoint: &Checkpoint) {
        self.height = checkpoint.height;
        self.root = checkpoint.root;
        self.trust = TipTrust::LocalApplied;
    }

    /// Light steady state: extend a verified chain by one step whose new
    /// head the caller trusts. O(1) per step.
    pub fn advance_chain(&mut self, step: TipStep, trusted_head: &Particle) -> Result<(), TipError> {
        if self.trust != TipTrust::ChainVerified {
            return Err(TipError::NotChainMode);
        }
        if step.height <= self.height {
            return Err(TipError::HeightRegression);
        }
        let prev = self.head.ok_or(TipError::NotChainMode)?;
        let head = link(&prev, &step);
        if head != *trusted_head {
            return Err(TipError::UntrustedHead);
        }
        self.height = step.height;
        self.root = step.root;
        self.head = Some(head);
        Ok(())
    }

    /// Export a checkpoint (no accumulator: the legacy one proved nothing).
    pub fn to_checkpoint(&self) -> Checkpoint {
        Checkpoint {
            root: self.root,
            acc: None,
            height: self.height,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TipError {
    NotChainMode,
    HeightRegression,
    UntrustedHead,
    EmptyChain,
}

/// Prover side: records every finalized height for export to light clients.
#[derive(Clone, Debug, Default)]
pub struct TipProver {
    chain: TipChain,
    head: Particle,
}

impl TipProver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one height. Call after the state root is fixed at `height`.
    pub fn push_height(&mut self, height: u64, root: Particle) -> Result<(), TipError> {
        self.push_block(height, root, [0u8; 32])
    }

    /// Append height + bbg root + root-leaves commitment.
    pub fn push_block(&mut self, height: u64, root: Particle, leaves: Particle) -> Result<(), TipError> {
        if self.chain.steps.last().is_some_and(|s| height <= s.height) {
            return Err(TipError::HeightRegression);
        }
        let step = TipStep { height, root, leaves };
        self.head = link(&self.head, &step);
        self.chain.steps.push(step);
        Ok(())
    }

    pub fn chain(&self) -> &TipChain {
        &self.chain
    }

    /// Current chain head (what a light client must already trust).
    pub fn head(&self) -> Particle {
        self.head
    }

    pub fn checkpoint(&self) -> Checkpoint {
        let last = self.chain.steps.last();
        Checkpoint {
            root: last.map(|s| s.root).unwrap_or_default(),
            acc: None,
            height: last.map(|s| s.height).unwrap_or_default(),
        }
    }

    /// The tip this prover's own chain verifies to.
    pub fn seal_tip(&self) -> Result<Tip, TipError> {
        if self.chain.steps.is_empty() {
            return Err(TipError::EmptyChain);
        }
        Ok(Tip::join_chain(&self.chain, &self.head))
    }
}

/// Bootstrap a one-step chain tip (tests / genesis).
pub fn join_with_demo_chain(root: Particle, height: u64) -> Tip {
    let mut prover = TipProver::new();
    prover.push_height(height, root).expect("first step");
    prover.seal_tip().expect("seal")
}

#[cfg(test)]
#[path = "tip_tests.rs"]
mod tests;
