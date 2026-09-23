---
tags: foculus, audit, rust, rewards, fold-mining
crystal-type: source
crystal-domain: cyber
---

# fold step cost: the "~30 field ops class" estimate against a real measurement

date: 2026-09-23 · revision: 177fad4 (origin/master) · rustc 1.98.0 (macOS arm64)

launch registry property #3 / #5. `docs/explanation/latency-targets.md`'s
clock-C table (light client join and tip follow) estimates:

```
| steady-state per tip | O(1) fold (~30 field ops class) + header | no re-decide from genesis |
```

launch #3 (2) (foculus#35, reviewed) reconciled the same table's
`decide(folding_acc)` verify line against a real measurement
(`audit/ticket-proof-cost.md`, foculus#6) and left the fold-cost line as
"an architectural estimate, not a measurement — a follow-up once a
per-block fold benchmark exists." this file is that follow-up.

## what "steady-state per tip" actually calls

`src/tickets.rs`'s `absorb_ticket` (fold a new ticket into an existing
accumulator) and `fold_acc` (merge two accumulators, the tournament-tree
step) are the two candidates. both:

- insert into / iterate `ClusterAcc.seen: BTreeSet<(miner, nonce)>`, whose
  size is `k`, not a constant;
- call `acc_commitment`, which rehashes the *entire* accumulator — every
  `sum_m` entry and every `seen` pair — on every single call, not just the
  newly absorbed ticket.

`k` is not unbounded: `ClusterAcc::k_min` (Hoeffding target for ε=0.1,
δ=0.01) is ~265, and settlement rewards turn off once `k ≥ k_min`
(specs/fold-mining.md "payment"). so this is a *bounded* cost, not the
unbounded-network-size case Assumption-G-style arguments worry about
elsewhere in this repo — but "bounded by a few hundred `BTreeSet` ops plus
one `Poseidon2`-class hash over the whole accumulator" and "~30 field ops"
are different claims, and the doc table does not distinguish them.

## measurement

`tests/fold_step_cost.rs`, 8 contributors (matching
`audit/hardware-profile.md`'s ε-support range), `k_min = 265`
(`ClusterAcc::k_min(0.1, 0.01)`), 2000 iterations per timing, real tickets
from `grind_settlement` with `target = u64::MAX` (every nonce wins, so the
timing measures the fold step itself, not the win-test):

```
$ cargo test --release --test fold_step_cost -- --nocapture
[fold-step-cost] fold_acc: two k=132 accumulators (k_min=265) -> 664967 ns/call
[fold-step-cost] absorb_ticket: cold(k=0->1)=10702 ns/call, steady(k=264->265)=656250 ns/call, ratio=61.32x
test result: ok. 2 passed; 0 failed
```

`absorb_ticket` at realistic steady-state size (k=264→265, the case a
light client repeats every tip once it has caught up) costs ~656 µs — 61×
its cost at k=0→1, confirming the per-call cost scales with `k`, not a
constant. `fold_acc` merging two k=132 accumulators into a k_min-sized one
costs ~665 µs, the same order as `absorb_ticket` at the same total k (both
pay for one `acc_commitment` rehash of a k_min-sized accumulator).

## reconciliation

~30 field ops on this crate's Goldilocks arithmetic is sub-microsecond —
five to six orders of magnitude below the ~0.66 ms measured. the estimate
undercounts because it prices the algebraic sum (`sum_m[i] += m[i]`, which
genuinely is O(1) per call) but not the `BTreeSet` bookkeeping or the
whole-accumulator rehash `acc_commitment` does on every call. even at the
measured ~0.66 ms, the fold step stays two orders of magnitude below
`decide`/`verify` (9–19 ms, `audit/ticket-proof-cost.md`) and four orders
below clock-A/B network stages (gossip ~0.4–3 s, φ* contraction ~1–4 s), so
`docs/explanation/latency-targets.md`'s conclusion — "verify is effectively
free compared to network" — still holds. only the specific "~30 field ops
class" line for the fold step itself does not.

## what remains

- update `docs/explanation/latency-targets.md`'s clock-C table with this
  measurement, in the same style foculus#35 used for the decide/verify
  line (done in this PR).
- `acc_commitment` rehashing the whole accumulator on every `absorb_ticket`
  call rather than an incremental update is itself an implementation
  choice, not a protocol requirement — an incremental commitment (e.g.
  folding the new ticket's contribution into the hash state rather than
  rehashing `sum_m` and `seen` from scratch) would cut steady-state
  `absorb_ticket` cost close to the cold-start ~12 µs baseline. that is a
  real code change to a hot, security-relevant function
  (`acc_commitment`/`fold_acc`/`absorb_ticket` all touch what `decide`
  ultimately proves over) and is out of scope for this measurement-only
  slice.
