---
tags: foculus, audit, rust, rewards, fold-mining
crystal-type: source
crystal-domain: cyber
---

# withholding bias bounded by compute share

date: 2026-09-19 · revision: 177fad4 (origin/master) · rustc 1.98.0 (macOS arm64)

launch registry property #7: "withholding bias bounded by compute share".
tru/specs/rewards.md §7 "Residual: withholding" argues informally that a
settlement miner who is also a cluster contender can withhold winning
tickets that would lower its own share, and that the resulting bias is
bounded by its compute share. specs/fold-mining.md §withholding bias derives
the exact bound this file measures against:
`bias(q) <= (mean - min) * q / (1 - q)`.

## measurement

`tests/withholding_bias.rs` grinds 400 real winning tickets over a 6
contributor cluster (`grind_settlement`, target = `u64::MAX >> 3`, real
`tru::impulse`-backed marginals throughout), assigns each ticket to a
simulated withholder at compute share `q` by a hash of its nonce
(independent of the ticket's marginal, so assignment does not itself bias
the sample), and has the withholder discard every assigned ticket whose own
marginal falls below the pool's true mean before self-folding.

```
$ cargo test --release --test withholding_bias -- --nocapture
[withholding-bias] q=0.05 bias=0.000062 bound=0.000106
[withholding-bias] q=0.10 bias=0.000145 bound=0.000224
[withholding-bias] q=0.25 bias=0.000470 bound=0.000671
[withholding-bias] q=0.50 bias=0.001333 bound=0.002012
test result: ok. 1 passed; 0 failed
```

measured bias stays at roughly 60-70% of the analytic bound at every `q`
tested, and is monotonically non-decreasing in `q` as the derivation
predicts. the run is fully deterministic (nonce-hash assignment, no wall
clock in the comparison), so these numbers reproduce exactly.

## what remains

this measures the bias a withholder can inject, not the two mitigations §7
names to price it: a withheld ticket forfeiting its subsidy (calibrated
above the share-gain this bound quantifies) and role separation (a miner
excluded from settling a cluster it contends in). both are still open
implementation work, listed in the registry against this property.
