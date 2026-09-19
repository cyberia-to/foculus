---
tags: foculus, audit, rust, rewards, fold-mining
crystal-type: source
crystal-domain: cyber
---

# progress-freedom across clusters — settlement difficulty banding

date: 2026-09-19 · revision: 177fad4 (origin/master) · rustc 1.98.0 (macOS arm64)

launch registry property #4: "progress-freedom across clusters: difficulty
schedule corrects per-cluster cost". specs/fold-mining.md §settlement
difficulty now specifies the fix; this file records the measurement that
motivated it and the numbers `tests/progress_freedom.rs` asserts against.

## the gap

`settlement::marginals` costs one `tru::impulse` call per contributor, so a
settlement attempt's wall-clock cost is linear in cluster size `n`. the
`settle-target` win-test was flat: every cluster shared one target regardless
of `n`, so win probability per attempt was equal but win probability per unit
of wall-clock time was not — a cluster two orders of magnitude larger than
another spent two orders of magnitude longer per attempt, and would
systematically miss `k_min` inside the settlement window.

## measurement

`tests/progress_freedom.rs` times `try_settlement_ticket` (target = 0, so
every attempt pays full marginal cost, no early winner exit) over 40 nonces
at `n = 4` and `n = 32`, then compares expected time-to-win under a flat
target against `tickets::banded_target`.

```
$ cargo test --release --test progress_freedom -- --nocapture
[progress-freedom] cost small(n=4)=0.000818s large(n=32)=0.010008s flat_ratio=12.24 banded_ratio=1.53
test result: ok. 1 passed; 0 failed
```

three repeated release runs: flat_ratio 11.85–12.24, banded_ratio 1.48–1.53.
debug build (less representative of production cost, included for reference):
flat_ratio 10.94, banded_ratio 1.37.

reading: attempt cost at n=32 is ~12x attempt cost at n=4 (close to the 8x
contributor-count ratio, plus fixed overhead). under the flat target,
expected time-to-win inherits that ~12x gap. under `banded_target`, the two
clusters' expected time-to-win differ by a factor of ~1.5, not ~12 — banding
the target linearly with `n` against a shared baseline restores
progress-freedom to within a small constant factor.

## what remains

this measures the fix at two cluster sizes on synthetic contributions with
one link each. still open for property #4 per the registry: a network
simulation on `million.rs`-style chaos infrastructure with many concurrently
mining clusters of varying size and a real epoch window, per the registry's
originally named evidence path — this file and the two-point measurement are
a first step, not the full simulation.
