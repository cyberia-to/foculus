---
tags: foculus, audit, fold-mining, zheng
date: 2026-09-19
---
# ticket proof cost vs the O(1) claim

[Property #3](https://github.com/cyberia-to/cyber/blob/master/launch.md) of
the phase-1 launch registry: ticket proof cost must be less than ticket
reward, measured on real `m(n)` with the folded proof. [specs/fold-mining.md](../specs/fold-mining.md)
claims the root decider is O(1) — roughly 825 constraints, 10-50us to
verify — independent of how many settlement tickets were folded into the
accumulator first.

`tests/ticket_proof_cost.rs` grinds real settlement tickets with
`grind_settlement` (an always-win target, so every attempt is a genuine
`m(n)` sample over the real `foculus::settlement` marginal computation),
folds them through `TicketProver::fold_settlement`, then times `seal`
(decide) and `verify_fold_seal` in isolation from the fold loop, at
n = 1, 8, 64, 512 tickets.

Validated with rustc 1.98.0 on macOS arm64, revision `177fad4` (origin/master):

- `cargo check --tests`: succeeds (after the two dependency-version fixes
  below; no other warnings introduced).
- `cargo test`: 144 lib tests, 17 attack_vectors, 1 million, 12 stress, 19
  structural_sync, 1 ticket_proof_cost — all pass, 0 failed.
- `cargo test --release --test ticket_proof_cost -- --nocapture`:

  | n tickets | fold (linear) | decide | verify |
  |---|---|---|---|
  | 1 | 999us | 45.2ms | 14.4ms |
  | 8 | 4.5ms | 21.2ms | 9.2ms |
  | 64 | 28.8ms | 18.7ms | 9.2ms |
  | 512 | 232.1ms | 18.7ms | 9.2ms |

## reading

decide and verify are flat from n=8 onward (n=1 carries one-time setup
cost) — the O(1) shape in the spec holds in this implementation: cluster
settlement verification cost does not grow with the number of folded
tickets, so a flood of minimum-cost tickets does not turn into an
asymmetric-cost DoS on the verifier. Fold itself is linear as expected
(~450us/ticket in release), paid once per ticket by whoever folds it, not
repeated at decide or verify time.

The absolute numbers are far from the spec's 10-50us estimate: measured
decide is ~18.7ms and verify ~9.2ms at steady state, roughly three orders
of magnitude higher. The spec number is a constraint-count estimate for
the SuperSpartan check alone; the measured number is the full
`zheng::decide` / `zheng::verify` call including HyperNova group
operations and setup that the constraint count does not price. This gap
is not yet closed — the constant is real and small in absolute terms
(single-digit milliseconds), but the property's evidence should read
"O(1), ~9-19ms measured" rather than "~10-50us", until the estimate is
reconciled with a profiled breakdown or the estimate itself is revised.

The reward side of the comparison — an actual $ or $CYB figure for what a
winning ticket pays — is not yet measurable: tier-2 fold subsidy amounts
depend on the allocation curve theta^alpha (property #27, open) and the
per-epoch security budget, neither fixed yet. What this measurement
closes is the cost side and the O(1) shape; what remains is pricing the
reward side once #26/#27 land, then comparing in the same unit (time
converted to compute cost, or a subsidy floor in tokens).

## remains

- reconcile the ~9-19ms measured constant against the spec's ~10-50us
  estimate (profile `zheng::decide`/`verify` to see where the gap is, or
  revise the spec number)
- reward-side figure once properties #26/#27 (allocation curve, staking
  budget split) are implemented, to close the comparison in one unit
- repeat at realistic cluster sizes (thousands of tickets) once fold-tree
  wiring (property #5) runs end to end on three nodes
