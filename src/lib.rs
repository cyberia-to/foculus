// ---
// tags: sync, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! foculus — the reconciliation engine: structural-sync substrate plus fork-choice.
//!
//! Two halves of one operation. The substrate agrees on non-conflicting facts;
//! fork-choice completes the merge on conflicts.
//!
//! Substrate (VEC layers):
//!   chain    — per-neuron hash chain + step counter + equivocation (P6)
//!   vdf      — verifiable delay function for layer-2 timing (P6)
//!   erasure  — Reed-Solomon over Goldilocks (P3)
//!   das      — data availability sampling (P3)
//!   nmt      — namespace Merkle tree (P2)
//!   store    — chunk store + CRDT (P1, file-level)
//!   node     — device sync daemon
//!   vdisk    — virtual disk manager
//!
//! Consensus (fork-choice — the merge completed):
//!   conflict  — conflict detection, a pure function of signal content (M1)
//!   fork      — the ForkChoice trait + Serialize / MinHash strategies (M1)
//!   reconcile — detection wired to resolution, incremental and batch (M1)
//!   (Focus strategy / φ*, domain-local finality, epoch pipeline — M2–M4)

pub mod chain;
pub mod vdf;
pub mod frames;

pub mod das;
pub mod erasure;
pub mod nmt;
#[cfg(feature = "net")]
pub mod node;
pub mod store;
pub mod vdisk;

pub mod cli;
pub mod conflict;
pub mod finality;
pub mod focus;
pub mod fork;
pub mod reconcile;
pub mod settlement;

pub use chain::{ChainError, CyberlinkRecord, Signal, SignalChain, SELF_NETWORK};
pub use vdf::{VdfProof, challenge_from_hash, evaluate as vdf_evaluate, verify as vdf_verify};
pub use frames::{
    decode_events, decode_signal_frame, decode_signals, encode_intent_frame, encode_signal_frame,
    CyberFrame, RENDER_BIN,
};

pub use conflict::{conflict_keys, ConflictGroup, ConflictIndex, ConflictKey};
pub use finality::{certified, crosses_threshold, finalizes, Domain, Finality};
pub use focus::Focus;
pub use fork::{ForkChoice, ForkError, GraphView, LinksView, MinHash, Serialize};
pub use reconcile::{Reconciler, Resolved};
