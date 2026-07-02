// ---
// tags: sync, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! foculus — structural sync substrate plus fork-choice.
//!
//! Owns the full sync stack: validity (layer 1), ordering (layer 2),
//! availability (layer 4), and local merge (layer 5-local). Layer 3
//! (completeness) is delegated to bbg.
//!
//! Modules:
//!   chain    — per-neuron hash chain + step counter + equivocation
//!   vdf      — verifiable delay function for layer-2 timing
//!   erasure  — Reed-Solomon over Goldilocks
//!   das      — data availability sampling
//!   nmt      — namespace Merkle tree
//!   store    — chunk store + CRDT
//!   node     — device sync daemon
//!   vdisk    — virtual disk manager

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

pub use chain::{ChainError, CyberlinkRecord, Signal, SignalChain, SELF_NETWORK};
pub use vdf::{VdfProof, challenge_from_hash, evaluate as vdf_evaluate, verify as vdf_verify};
pub use frames::{
    decode_events, decode_signal_frame, decode_signals, encode_intent_frame, encode_signal_frame,
    CyberFrame, RENDER_BIN,
};
