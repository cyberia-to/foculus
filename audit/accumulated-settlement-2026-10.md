---
tags: foculus, audit, fold-mining, accumulation, zheng
crystal-type: audit
crystal-domain: cyber
---
# cluster settlement on zheng accumulation — 2026-10-09

Phase 3 of the proof-system repair (soft3 `proposals/proof-system-repair.md`
§4–§5), foculus side: a cluster's tickets as statements of zheng
`accumulate::ccs`, one accumulation and one decider per cluster, verified
against the native per-ticket path. Contract: `specs/fold-mining.md` § proof
profile today. Code: `src/cluster_accumulated.rs` (feature
`accumulated-settlement`, off by default), `examples/accumulated_bench.rs`.

Stand: foculus `feat/accumulated-settlement` on `feat/sound-settlement-proofs`
(#61); zheng, lens, joy `feat/accumulation` (zheng#51); hemera
`perf/permutation-neon`; bbg `feat/sound-proof-consumers`; nox, trident
`release/0.4`. Apple M4 Max, 16 cores, rustc 1.95.0, `--release`.
The machine was shared with other agents (load average 22–36) during every
run: times are single runs (verify: median of 3) and upper bounds of a quiet
machine; native times here are ~1.8× the quiet-machine numbers of
`sound-settlement-proofs-2026-10.md` §5.2. Sizes are deterministic.

## 1. what was built

- `prove_cluster_accumulated`: statement checks and duplicates as in
  `cluster::verify_cluster`; each ticket's `TicketRelation`
  (`m[π_j] = v_j − v_{j−1}`) becomes one `accumulate::ccs::Statement` with the
  ticket's statement digest as its bytes; `ccs::fold` runs every statement's
  Spartan IOP down to its witness claim, accumulates `chunk` words per step and
  decides once.
- `verify_cluster_accumulated` = `verify_folded_part` (statement checks,
  duplicates, `ccs::verify_folded`) + `check_values` (native impulse
  recomputation of every disclosed coalition value). Returns the same
  `ClusterAcc` as the native path.
- Parameters `AccParams::settlement()`: lens default WHIR (rate 1/16, folding
  4, Johnson, grinding 16), 64 statements per step. Protocol constants, not
  carried in the proof.
- Native `cluster::verify_cluster` is unchanged and stays the default.

## 2. what the accumulated proof establishes — and what it does not

No CCS for `tru::impulse` exists, so the coalition values `v_j` cannot be a
committed witness: the verifier must read them to recompute impulse. They are
disclosed and pinned, so every column the ticket relation reads is public and
the accumulated proof proves rows the verifier could check by subtraction. The
accumulated profile is therefore not more sound than the native one and not
cheaper: the native recomputation is still all of a ticket's real cost. What it
delivers is the plumbing a succinct cluster needs: once impulse has a relation,
the values and the impulse trace become the committed witness, `check_values`
disappears, and the same `prove`/`verify` pair carries the cluster.

## 3. measurements

`cargo run --release --features accumulated-settlement --example accumulated_bench`.
Fixture of `settlement_bench`: a ring of particles with chords; n contributors,
one link each. Native B = public-profile proof bytes of all tickets;
accumulated B = `Folded::to_bytes` + 8 B per disclosed value, split as
per-statement claim proofs / accumulation steps / decider / values.

| tickets | n | base links | chunk | native prove ms | native verify ms | native B | acc prove ms | acc B | claims / steps / decider / values | acc verify ms | folded part ms | native values ms |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 16 | 3 | 6 | 16 | 66.5 | 30.2 | 976 | 226.4 | 29,597 | 8,448 / 17,272 / 3,485 / 384 | 19.0 | 5.3 | 10.2 |
| 512 | 3 | 6 | 64 | 771.0 | 636.0 | 31,232 | 1,910.5 | 850,628 | 270,336 / 564,511 / 3,485 / 12,288 | 704.7 | 257.4 | 474.2 |
| 512 | 3 | 6 | 16 | 506.2 | 486.4 | 31,232 | 2,436.2 | 921,212 | 270,336 / 635,095 / 3,485 / 12,288 | 766.0 | 250.8 | 504.5 |
| 16 | 8 | 128 | 16 | 2,201.9 | 2,442.8 | 1,616 | 1,445.5 | 82,533 | 11,328 / 57,304 / 12,869 / 1,024 | 1,225.5 | 41.1 | 1,360.6 |
| 512 | 8 | 128 | 64 | 21,738.0 | 26,331.9 | 51,712 | 26,329.7 | 2,356,684 | 362,496 / 1,948,543 / 12,869 / 32,768 | 33,590.9 | 704.8 | 35,471.3 |

The 16-ticket native verify at n = 3 (30.2 ms) is above its accumulated total
(19.0 ms) only through load noise: both run the same impulse recomputation,
and the native path's own CCS check is microseconds (§5.1 of the 2026-10
settlement audit: 0.0017 ms per ticket at n = 3).

Reading:

- bytes: the accumulated cluster is 27× (n = 3) to 46× (n = 8) larger than
  the per-ticket public proofs at 512 tickets. A claim proof is 528 B (n = 3)
  to 708 B (n = 8) per ticket; the accumulation steps (spot-check openings of
  every folded word) dominate. Only the decider is small: 3.5 KB (n = 3) and
  12.9 KB (n = 8), one per cluster.
- verify: the folded part costs 257 ms (n = 3) and 705 ms (n = 8) for 512
  tickets on top of the native recomputation, which it cannot replace (§2). At
  n = 8 the recomputation (tens of seconds) dominates both paths.
- prove: the settler's extra work is the Spartan IOP per ticket, the steps and
  the decider: +1.1–1.9 s at 512 tickets, n = 3.
- chunk 16 vs 64: more steps cost more bytes (+70 KB); verify unchanged within
  noise.

## 4. verdict

Accumulated verification is slower than native recomputation today and its
proof is larger: native per-ticket verification stays the default, the
accumulated path ships behind the `accumulated-settlement` feature. It becomes
the default when both hold: (a) impulse has a relation, so the native
recomputation leaves the verifier; (b) zheng IVC (soft3 plan, work package R1)
checks each step's openings inside the next step, so the cluster proof is one
accumulator plus one decider instead of every step's openings. Without (b) the
proof stays linear in tickets (zheng `audit/accumulation-2026-10.md` § why size
is linear).

## 5. tests

`src/cluster_accumulated_tests.rs` (with the feature), differential against
`cluster::verify_cluster`:

- an honest cluster of 10 tickets in three steps (the last partial) produces
  the native accumulator (k, sums, commitment, seen set);
- a telescoping forgery — one marginal raised, every coalition value shifted so
  the rows hold, re-committed and re-scored, folded honestly by a dishonest
  prover — passes the folded proof and is caught by the native value check
  (`Part::Native`), as natively;
- a disclosed value changed after proving, a forged witness claim, a dropped
  step, swapped tickets, a dropped ticket, another beacon: rejected by the
  folded part;
- duplicates, a wrong score, an empty cluster: the native errors.

Full suite with and without the feature: §6.

## 6. suite

`cargo test --release --features accumulated-settlement`: 235 passed, 0
failed (lib 170, attack_vectors 17, legacy_frames_strict 6, million 1,
no_legacy_zheng 2, signal_codec 8, stress 12, structural_sync 19). Without the
feature: lib 166 passed. `cargo clippy --release --all-targets --features
accumulated-settlement`: no warning in a touched file. The legacy-path guard
(`tests/no_legacy_zheng.rs`) allows `zheng::accumulate::ccs`, the checked
successor of the legacy fold.

## 7. not done

- a relation for `tru::impulse` (tru) — the precondition for the accumulated
  profile to carry soundness of its own;
- zheng IVC (soft3 plan R1) — the precondition for a constant-size cluster
  proof;
- the fold lottery's steps (`σ_f`) as accumulation steps between miners: a
  settler accumulates a whole cluster in one pass; an accumulator does not
  travel between parties yet (zheng has no wire form of an accumulator
  instance);
- `settle_tickets` receipts and the signal codec still carry the public
  profile only.
