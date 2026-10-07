# foculus

the reconciliation engine of [[soft3]]: structural-sync substrate plus the fork-choice that completes the merge. `src/lib.rs` lists the modules; `specs/` is normative, `audit/` is evidence.

## boundary

one responsibility: **agree** — on order, on availability, on the winner of a conflict. foculus owns:

- layer 2 ordering — `chain` (per-neuron hash chain, step, equivocation), `vdf` ([[delay]]); cybergraph re-exports and calls them, never reimplements
- layer 4 availability — `erasure`, `das` ([[erasure]]); radio transmits shards, hemera hashes them
- layer 5 merge — `store`/`node` (device-level CRDT), `conflict`/`fork`/`reconcile` (fork-choice), `finality`, `epoch`, `beacon`, `tickets`, `settlement`
- position — [[place]] (spec only)

foculus consumes and never re-derives:

- φ\* and the tri-kernel — [[tru]] (`tru/specs/tri-kernel.md`, `focusing.md`, `proving.md`). a foculus page may *use* φ\*; it may not restate the kernel, its contraction rate or its circuit
- hashing, Merkle/NMT node rules — [[hemera]] (`hemera::tree::hash_node_nmt`)
- state and openings — [[bbg]], [[lens]]
- proofs — [[zheng]]
- framing — [[tade]]; transport — [[radio]]
- confidentiality and keys — [[mudra]]; subject identity — [[neuron]]

the duplication ledger that fixed these borders is `soft3/roadmap/component-boundaries.md`. when a new file would implement a mechanism another repo owns, add a call, not a copy.

## rules

- field arithmetic only in anything that can be proven or must agree across machines — no `f64` outside display code (`src/cli.rs` prints; it does not decide)
- every protocol constant lives in `specs/parameters.md`, referenced symbolically elsewhere
- `cargo test` green and zero warnings before a commit; never commit to `master` directly — branch + PR
- plans and audits go under this repo (`audit/`, `.claude/plans/`), never under `~/.claude/`
