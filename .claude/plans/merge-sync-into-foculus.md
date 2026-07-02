# merge sync into foculus

status: executing. the reconciliation engine and the fork-choice that completes it become one crate.

## why

`sync`'s `GSet::merge` is a pure CRDT union — it keeps *both* members of a conflict, undefined on collision. foculus's fork-choice is the completion of that merge on conflicts. they are two halves of one reconciliation operation kept in separate repos; `sync` already does conflict *detection* (equivocation, `chain.rs`) while resolution lives nowhere. merging makes the merge function whole and gives one canonical `Signal` type instead of the drift between `sync::Signal`, `bbg`'s copy, and foculus's paper spec.

decided with the user: fork-choice is a pluggable strategy, not φ*-always-on. the trusted/single-writer path must never be forced to compile the tri-kernel. and resolution supports **both incremental (per-conflict-pair) and batch-at-epoch (per-domain) timing** for the foreseeable future — not one or the other.

## scope (confirmed: all of sync)

sync self-describes as "device data availability — erasure-coded virtual filesystem" but holds the full VEC substrate: chain (P6), GSet::merge (P1), nmt (P2), das+erasure (P3), vdf, plus vdisk/store/node device-DA and optional iroh transport. moving all of it makes foculus the full node crate = VEC substrate + fork-choice. consistent with "foculus = structural sync with its merge completed."

## moves

code (sync → foculus root, keeping crate at root as sync had it, so `../` dep paths stay valid):
- Cargo.toml → foculus/Cargo.toml, rename package + bin `cyber-sync` → `foculus`
- Cargo.lock, .gitignore, src/, tests/ → foculus/
- specs/sync.md → foculus/specs/structural-sync.md (the substrate spec, complements protocol.md)
- specs/README.md → dropped (foculus/README.md already indexes specs)
- NOT target/, NOT .git

rewire dependents (crate rename in the same commit — the "eat the cost" the user chose):
- cybergraph/Cargo.toml, cybergraph/cli/Cargo.toml, cyb/core/Cargo.toml: `cyber-sync {path=../sync}` → `foculus {path=../foculus}`
- cybergraph/src/lib.rs, cybergraph/src/api.rs, cybergraph/cli/src/main.rs, cyb/core/src/cell.rs: `use cyber_sync` → `use foculus`

markdown project references → foculus (8 files, 5 repos): honeycrisp/acpu/README, cybergraph/specs/cli, bbg/specs/cli, soft3/{roadmap/component-boundaries, roadmap/stack-completeness, specs/terms, proposals/cybergraph-sync-tape-architecture}, go-cyber/docs/upgrade-plan. per-file judgment — change crate/path/project refs, not the word "sync" where it means the general concept.

## order (build-verify before the irreversible step)

1. move code + rename crate
2. rewire dependents
3. `cargo check` foculus (no-default-features, skip iroh) then cybergraph — must pass
4. commit the move in foculus + the rewiring in cybergraph/cyb
5. markdown pass
6. only then: `rm -rf ~/cyber/sync`

## known costs, accepted

- sync's git history does not follow into foculus (pragmatic file move, not subtree merge). recoverable from the sync repo until step 6.
- foculus stops being specs-only; it is now a code repo with specs/ alongside src/. the sibling `rs/` convention (bbg, nox) is not followed — a faithful move over a restructure. aligning to `rs/` later is a separate, optional cleanup.

## next after merge (not this pass)

the `ForkChoice` trait itself — `Serialize` / `MinHash` / `Focus` strategies, both incremental and batch resolution timing. captured in structural-sync.md / protocol.md as the design, scaffolded when the reconciliation entry point is wired to call it.
