# foculus implementation plan

status: M1–M4 done, M3-wire done, CLI done. consensus layer went from 0% to substantially implemented (all with tests, zero warnings). remaining: M5 (mostly stale on audit — see below), CLI verdict surfacing, cross-repo Signal unification.

## progress (all committed, green)

- M1 — fork-choice foundation (conflict, fork, reconcile; Serialize/MinHash)
- M2 — Focus strategy (φ* via tru's tri-kernel)
- M3 — finality primitives (ε-support domain, sqrt-free adaptive threshold, P2 gate)
- M3-wire — resolve_and_finalize + Reconciler<Focus>::finalize_all (one φ* → winner + verdict)
- M4 — support-switching (T1 drift) + epoch beacon (b_E = VDF over finalized set)
- settlement (parallel) — Shapley lottery, Monte-Carlo ↔ exact
- CLI — 7 commands, cyber house-style palette + logo

## M5 audit finding (the flagged issues were mostly stale or conflated)

- **kill f64 — already done for what matters.** the whole consensus path (fork-choice, finality, φ*, switching, beacon, settlement) is `tru::Fx` fixed-point end to end — no float. the `store::GSet` "f64 confidence" the hygiene note flagged does not exist; `FileEntry` is LWW by integer `u64` timestamp. remaining `f64` (vdisk rebalancing ratios, `das::confidence` report, byte display) is off the consensus/proof path — the mandate's allowed boundary. corrected the stale note in `soft3/roadmap/component-boundaries.md`.
- **LWW→CRDT — a non-issue, it was a layer conflation.** `store::GSet` LWW is the *virtual-filesystem* file merge (last-writer-wins is correct for files). the *signal*-consensus merge `vec.md` P1 describes is implemented as conflict-detection + fork-choice (M1/M2), not LWW. two different layers; no divergence to reconcile.
- **nullifier — DECISION.** double-spend detection needs the nullifier, which lives in bbg's `Signal` (box_moves), not foculus's `Signal`. decision: bbg owns the nullifier double-spend (its structural `InsertError::DoubleSpend`); foculus owns signal-level equivocation + fork-choice. the conflict machinery is generic over `ConflictKey`, so when the two Signal shapes are unified (a cross-repo change, not foculus-unilateral), adding the double-spend key is additive — `conflict_keys` gains one line. until then double-spend is a bbg-boundary concern, not a foculus gap.

## the original map follows

status: M1 in progress. the map from spec (complete, adversarially reviewed) to code (substrate-only today).

## where we start

grounded in the completeness report: ~3,900 lines of tested substrate (chain/vdf/nmt/das/erasure/store/vdisk/node/frames) inherited from the sync merge, and 0 lines of the consensus layer. the substrate implements VEC P2/P3/P6 and a file-level LWW merge for the virtual filesystem — it is *not* signal-level reconciliation, and the fork-choice seam does not exist yet. everything from `protocol.md` step 4-6, `security-at-scale.md` (L1/L2/T1/T2/S4/S5), `beacon.md`, `fold-mining.md` is greenfield.

the design is settled (prior sessions): one reconciliation engine, fork-choice as a pluggable strategy — `Serialize` (single-writer), `MinHash` (trusted multi-writer), `Focus` (trustless φ*). resolution supports both incremental (per-conflict) and batch-at-epoch (per-domain) timing. the trusted path must never compile the tri-kernel — `Focus` is feature-gated on `tru`.

## key constraint the code forces

foculus's `Signal` (neuron, network, links, delta_pi, prev, step, height, proof) carries **no nullifier**. so:
- equivocation (same neuron + step, divergent content) is detectable now — the data is present.
- double-spend and resource-collision need the nullifier / resource-id, which the signal does not carry. either add a nullifier field to `Signal` (wire-format change, ripples to bbg/cybergraph) or delegate double-spend detection to bbg (which already has `InsertError::DoubleSpend`) and have foculus consume its verdict. this is an M5 decision; M1 builds the conflict/fork-choice machinery generic over the conflict key so it is unaffected by which way that goes.

## milestones (dependency-ordered)

### M1 — conflict model + ForkChoice trait + trivial strategies  [first shippable]
`src/conflict.rs`:
- `ConflictKey` (32 bytes: a nullifier, or hash(neuron ‖ step) for equivocation)
- `ConflictGroup { key, members: Vec<SignalId> }`
- `conflict_keys(&Signal) -> Vec<ConflictKey>` — pure function of content; equivocation implemented, double-spend a documented extension point (needs nullifier)
- an index (key → members) that accretes as signals arrive, so detection is incremental and monotonic (matches protocol.md's "monotonic local conflict index")

`src/fork.rs`:
- `trait ForkChoice { fn resolve(&self, members: &[Signal]) -> usize; }` — deterministic, returns winner index
- `Serialize` — single-writer invariant: a conflict is a logic error, returns an explicit error/marker rather than silently picking
- `MinHash` — lowest `Signal::hash()` wins; this is protocol.md's own measure-zero tiebreak, promoted to the general trusted rule

`src/reconcile.rs`:
- the engine that ties detection to resolution, in both timings:
  - incremental: on each new signal, update the conflict index; if it joins a group, resolve immediately
  - batch: accumulate an epoch's groups, resolve all at close
- both call the same `ForkChoice::resolve`

tests: equivocation detected, MinHash determinism (same set → same winner regardless of insertion order), Serialize rejects conflict, incremental vs batch produce identical winners.

deliverable: a working trusted-cloud consensus (Serialize/MinHash) with no φ* dependency.

### M2 — Focus strategy (feature-gated `focus`, deps tru)
map `CyberlinkRecord` → tru `Link::stake(from, to, amount)`, build `FocusingGraph::build(links, ctx)`, `compute_focusing(g, p)`, resolve a conflict group by higher `φ*` of its members. the trait's `resolve` gains the graph view it needs (Serialize/MinHash ignore it). `Cargo.toml`: `tru = { path = "../tru/rs", optional = true }`, `focus = ["dep:tru"]`.

### M3 — domain-local finality (L1/L2)
ε-support domain construction (the canonical superlevel set), adaptive threshold `τ_D = μ_D + κ'σ_D` over the domain, and the `Φ_uncert` certification gate using the existing `nmt.rs` P2 machinery. this is protocol.md step 6 — the finalization condition — made real.

### M4 — support-switching (T1) + epoch pipeline
switching rule (consensus-only signal, VDF-rate-limited via the existing `vdf.rs`), epoch beacon (outer VDF over the finalized set, `beacon.md`), settlement fold-mining (`fold-mining.md`). this is the full epoch machinery and the largest milestone.

### M5 — substrate reconciliation with the current spec
- `store.rs` LWW-Element-Set → the G-Set + topological-sort merge `vec.md` P1 specifies (or update vec.md if LWW is the deliberate choice for the vfs layer specifically — decide, don't leave two merge definitions)
- remove the `f64` confidence + clock-drift float per the field-arithmetic mandate
- resolve the nullifier wire-format decision (add field vs delegate to bbg)

## verification discipline (per every milestone)

`cargo check` + `cargo test` green before commit; the substrate's 19 existing tests must stay green throughout (no regression in the DA/sync layer while the consensus layer is built on top). property tests for the invariants that matter: determinism (same signals → same winner on every node), monotonic detection (a conflict once seen stays seen), conservation where applicable.

## honest scope

this is 6-8 focused sessions by the project estimation model, not one. M1 is self-contained and shippable on its own (trusted deployments). each later milestone is independently reviewable. the specs are complete and reviewed, so implementation is unblocked on design — the work is translation, not invention.
