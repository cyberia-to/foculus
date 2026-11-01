---
tags: foculus, audit, zheng, proofs, latency
date: 2026-10-14
---
# which proof each foculus statement carries — measured 2026-10-14

Question (proof-latency gate, soft3 `audit/proof-latency-2026-10.md`):
the user-device profile is zheng's profile 1 (one relation, committed
witness, Spartan over Fp3 + one WHIR opening, `device_params`, held key,
≤ 1 s). Should foculus's pay σ, settlement seal, fold-tree seal and light
tip move onto it, or stay where `audit/sound-settlement-proofs-2026-10.md`
put them (public certificate v3, or recomputation)?

Starting point: none of the four uses zheng's legacy fold/decide any
more — that move is foculus#61 (this branch's base), not this change.
The soft3 audit read the default branch, where they still did.

Stand `~/cyber/.stands/PW`: foculus `feat/sound-settlement-proofs`
87603ec + this branch, zheng `release/0.4` aa54bdd (zheng#64 merged),
lens `release/0.2`, nox/joy/trident `release/0.4`, bbg
`chore/coordinated-release-20260916` ccda7a2 (bbg#37: no legacy
accumulator), honeycrisp `main` 9cbd480. `cargo tree -e features -i zheng`:
only `default` — no `legacy` anywhere in the graph (now a test,
`tests/no_legacy_zheng.rs::zheng_builds_without_the_legacy_feature`).
Apple M4 Max, rustc 1.95.0, `--release`. **Load average 25–32 from other
agents during the runs**; profile-1 grinding (16 bits) adds variance.
Command: `ZHENG_BACKEND={cpu,metal} cargo run --release --example
device_profiles` (`DP_REPS=5`, medians).

## what changed in code

- `proof::Proof::Succinct` (profile byte 2): zheng profile 1 over the
  same `Relation` the verifier derives, `prove_succinct(&RelationKey, &r)`
  / `verify` / `verify_with_key`. The transcript absorbs a domain, the
  relation's hemera digest and the statement digest.
- `RelationKey`: held per relation shape — compiled CCS, pinned
  positions, relation digest, `device_params(ℓ)`. A key of another shape
  is refused.
- `Relation::closed` (default `false`): profile 1 is admitted only for a
  relation with no native witness check, since a committed witness cannot
  be read back. The ticket relation is not closed (coalition values are
  checked against `tru`'s impulse natively), so it can never be proven
  succinctly by omission.
- pay: the legs are the signal's own link amounts, public on the wire, so
  they are now pinned coordinates and the relation is closed (the native
  leg check is gone; v3 proofs are 8 B per leg smaller).
- signal codec v2 still carries profile 1 (public) only; it refuses to
  write a profile-1 proof (no statement chooses one, below).

## measurements

### pay σ (closed: legs pinned)

| legs | native recompute | v3 prove · verify · bytes | p1 ℓ | p1 cold · warm · verify · bytes (CPU) | p1 cold · warm · verify · bytes (Metal) |
|---|---|---|---|---|---|
| 1 | 14 µs | 0.04 · 0.03 ms · 701 B | 7 | 25 · 24.5 · 1.5 ms · 10,250 B | 11.3 · 10.6 · 1.4 ms · 10,250 B |
| 2 | 22 µs | 0.07 · 0.05 ms · 1,709 B | 8 | 12.4 · 10.6 · 2.1 ms · 10,850 B | 5.5 · 4.5 · 2.1 ms · 10,850 B |
| 16 | 122 µs | 0.36 · 0.30 ms · 15,733 B | 11 | 58 · 48 · 10.9 ms · 17,930 B | 26 · 16.6 · 11.0 ms · 17,930 B |
| 64 | 477 µs | 1.66 · 1.18 ms · 63,749 B | 13 | 127 · 95 · 39 ms · 36,118 B | 76 · 38.6 · 39.3 ms · 36,118 B |

v3 numbers are the same on both backends (no PCS). "native recompute" is
`PayStatement::of` (content id hash + checked sum): what a verifier
holding the signal does anyway.

### settlement ticket and seal

| statement | native / v3 (both backends) | profile 1 |
|---|---|---|
| ticket, n=3, 6 base links | prove 0.70 · verify 0.70 ms · 61 B | not sound (relation not closed). Relation alone: Metal 2.5 ms warm, verify 0.4 ms, 2,346 B — and the verifier would still recompute the impulse |
| ticket, n=16, 512 base links | prove 191 · verify 191 ms · 165 B | relation alone: 202 ms warm (the witness *is* the impulse), verify 0.7 ms + the same 191 ms of impulse, 6,658 B |
| settlement seal, 4 tickets (n=3) | verify 2.5 ms (4 impulses + hashes) · 212 B | 4 × the ticket row |

### fold-tree merge and tip step (hash statements, no witness)

| statement | native recompute | profile-1 alternative: nox hash of the same field-element count, zheng `ProvingKey` |
|---|---|---|
| fold-tree merge (`verify_fold_ticket`) | 33 µs | 12 inputs, ℓ=13: CPU cold 132 · warm 115 · verify 18.7 ms; Metal cold 73 · warm 54 · verify 19.2 ms; 37,987 B |
| tip step (`Tip::join_chain`, per step over 10k) | 8.5 µs | 13 inputs, ℓ=13: CPU cold 112 · warm 93 · verify 20.1 ms; Metal cold 67 · warm 47 · verify 19.8 ms; 37,774 B |

## choice and why

Every one of the four statements is fully public: the verifier holds all
inputs. Profile 1's verifier recompiles the relation from them and
evaluates its matrices (no preprocessing/holography in zheng), so its
verify cost is never below the size of the computation, and it adds a
WHIR opening. It buys bytes (a committed instead of a disclosed witness)
and, in the zk profile, hiding — neither is what these statements need.

| statement | carries | why (numbers above) |
|---|---|---|
| pay σ | **v3** | 27–600× faster to prove (warm key), 33–47× faster to verify than profile 1 at every leg count; smaller up to ~20 legs. At 64 legs p1 is 36 KB vs 64 KB, but costs 39 ms per verifying relay instead of 1.2 ms. Recomputation is the floor (14–480 µs) and v3 sits within 3× of it. |
| settlement seal (tickets) | **v3 + recomputation** | profile 1 is unsound for the ticket relation until the impulse has a CCS; even then the verifier cost stays the impulse. |
| fold-tree seal | **recomputation** | 33 µs native vs 19 ms verify + 38 KB for a profile-1 proof (600× slower). |
| light tip | **recomputation** | 8.5 µs per step native vs 20 ms per step verify + 38 KB. A constant-cost tip needs IVC/wrap (profiles 4–6, aggregator side), not profile 1. |

All four are within the ≤ 1 s device gate today on the prover side
(they cost the device microseconds to milliseconds); profile 1 would put
50–130 ms on each.

Where profile 1 (or veil) does belong: a statement whose witness is large
or private — the impulse once it has a CCS (succinct settlement, phase 3),
P1/P2 private authorship and amounts (mudra, veil). The `Succinct`
variant, the held key and the `closed` gate are the slot for those.

## zheng API wanted (not changed here)

`succinct::ProvingKey` holds zheng's column layout and relabelled relation
for nox programs only. For a caller-built CCS, `succinct::prove_relation`
re-derives them every proof, so `RelationKey` holds the compiled CCS, its
digest and parameters but not those. Wanted: `ProvingKey::for_relation(
instance, pinned_positions)` + `prove_relation_with_key`, the same bytes
as `prove_relation`. Also a public decoder for `succinct_proof_bytes`
(`AnySuccinct::from_proof_bytes`) if a profile-1 proof is ever to travel
in a foculus codec.

## not done

- profile 1 on the wire (codec): no statement chose it.
- succinct settlement (impulse CCS): phase 3 of the proof repair.
- the v3 pay certificate discloses 62 bits per leg and running sum as
  u64s (63.7 KB at 64 legs); packing them is a v3 codec question.
