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
pub mod frames;
pub mod signal_codec;
pub mod vdf;

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
pub mod finality_evidence;
pub mod focus;
pub mod fork;
pub mod beacon;
pub mod epoch;
pub mod epoch_cert;
pub mod live;
pub mod gossip;
#[cfg(feature = "net")]
pub mod radio_settle;
pub mod pay_proof;
pub mod phi;
pub mod proof;
pub mod wire;
pub mod reconcile;
pub mod rewards;
pub mod settlement;
pub mod settle_tickets;
pub mod cluster;
pub mod ticket_proof;
pub mod tickets;
pub mod tip;

pub use beacon::{
    advance_empty, beacon, claims_root, collect_signal_outputs, open_beacon, signal_vdf_root,
    verify_beacon, BeaconArtifact, DEFAULT_OUTER_T, GENESIS_PREV, TEST_OUTER_T,
};
pub use epoch::{verify_live_receipt, EpochError, EpochPhase, EpochRunner};
pub use epoch_cert::{
    issue_epoch_cert, verify_epoch_cert, verify_epoch_cert_with_phi, EpochCertificate,
    PhiVerifyInputs, ReceiptError, SettleVerifyInputs, verify_settlement,
};
pub use cluster::{
    ClusterError, ClusterProof, accept_tickets, cluster_acc, redraw_peer, verify_cluster,
    verify_fold_ticket,
};
pub use phi::{PhiBinding, PhiClaim, PhiGraph, compute_phi_star, verify_phi_claim};
pub use proof::{Proof, ProofError, PublicProof};
pub use live::{LiveError, LiveNode, LiveSignal, NodeMode};
pub use gossip::{
    decode_self_acc, encode_self_acc, SettleMesh, SettleMsg,
};
pub use wire::{
    claim_announce, decode_settle_msg, encode_frame, encode_settle_msg, split_frame,
    topic_from_claims_root,
};
#[cfg(feature = "net")]
pub use radio_settle::{RadioSettleSession, SettleRadio, SETTLE_ALPN};
pub use chain::{BoxMoveRecord, ChainError, CyberlinkRecord, SELF_NETWORK, Signal, SignalChain};
pub use finality_evidence::{FinalityEvidence, FinalityKind};
// issue_from_domain is on FinalityEvidence
pub use frames::{
    CyberFrame, RENDER_BIN, decode_events, decode_signal_frame, decode_signals,
    encode_intent_frame, encode_signal_frame,
};
pub use pay_proof::{
    MAX_PAY_AMOUNT, PayProofError, PayStatement, check_pay, check_signal_proof, nullifier_set_hash,
    prove_pay, verify_pay,
};
pub use rewards::{
    claim_from_links, mint_receipt_to_ledger, settle_epoch, settle_epoch_tickets,
    settle_with_peer_accs, share_of, valence_to_prediction, verify_receipt, RewardClaim,
    RewardError, SettleReceipt, SettledShare, TicketPolicy, DEFAULT_EMISSION_SCALE,
};
pub use settle_tickets::{ClaimSet, TicketSettlement, settle_cluster_tickets, ticket_receipt};
pub use ticket_proof::{
    ProvenTicket, TicketError, TicketInputs, TicketRelation, prove_ticket, verify_ticket,
};
pub use tickets::{
    absorb_ticket, assemble_fold_tree, commit_marginals, easy_target, fold_acc, grind_fold,
    grind_settlement, self_fold, try_settlement_ticket, verify_settlement_ticket, ClusterAcc,
    FoldTicket, SettlementTicket, DEFAULT_SETTLE_TARGET,
};
pub use tip::{Tip, TipChain, TipError, TipProver, TipStep, TipTrust, join_with_demo_chain};
pub use vdf::{VdfProof, challenge_from_hash, evaluate as vdf_evaluate, verify as vdf_verify};

pub use conflict::{ConflictGroup, ConflictIndex, ConflictKey, conflict_keys};
pub use finality::{Domain, Finality, certified, crosses_threshold, finalizes};
pub use focus::Focus;
pub use fork::{ForkChoice, ForkError, GraphView, LinksView, MinHash, Serialize};
pub use reconcile::{Reconciler, Resolved};
