---
tags: foculus, audit, receipt, proofs, proof-system-repair
date: 2026-11-18
---
# stale proof claims over foculus — gate receipt, 2026-11-18

Receipt for the phase-5 gate of the [proof-system
repair](https://github.com/cyberia-to/soft3/blob/master/proposals/proof-system-repair.md)
(§5, the `delete` row; the ledger of hits is §C): `scripts/stale-proof-claims.nu`
returns zero over this repository. Work package H rewrote foculus's stale
proof-system claims; this page records the run, so the claim is checkable by
someone who did not make the change.

## stand

| | |
|---|---|
| foculus | `docs/proof-repair-ledger` at `26f5849`, with this page present in the worktree (the run was repeated with it and still returns zero) |
| the script | soft3 `scripts/stale-proof-claims.nu` at soft3 `677efd2` (2026-10-10) — it lives in soft3, not here |
| nu | 0.112.2 |
| machine, time | Apple M4 Max, macOS 26.4, 2026-11-18T15:34Z |

## command and output

```
$ nu ~/cyber/soft3/scripts/stale-proof-claims.nu --root <path to this worktree>
stale-proof-claims: 0 hits — the ledger is closed
$ echo $?
0
```

`--root` is the workspace the script greps. Pointed at this worktree it covers
foculus alone: 91 files matching the script's `*.md`, `*.rs`, `*.toml`, `*.nu`
filters, with `target/` and `.git/` excluded as the script excludes them. None
of the script's allow-list entries is a foculus path, so nothing here is
exempted from the patterns.

## what the gate does and does not cover

It covers the pattern families listed in §C of the proposal and written out
as regular expressions at the top of the script: the opening and proof sizes
the old design advertised, the verify times it advertised, the name of the old
folding scheme, the claim that the commitment scheme opened without hash paths,
the retired constraint and field-operation counts, the claim that the
accumulator was itself the proof, and the retired soundness figures. The exact
strings are in the script; they are deliberately not quoted here, because a
page that quoted them would itself be a hit.

It does not cover the design vocabulary: it has no pattern for "folding
accumulator", for calling σ a "recursive proof", or for crediting accumulation
alone with constant size. Those are read, not grepped. The review of
`94402b6` found three such places left; they are rewritten in `26f5849`.

Zero hits here is therefore a necessary condition for the ledger's foculus
rows, not a sufficient one.
