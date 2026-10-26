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

## 2026-10-26 — the bound at the realized share

the test above asserted `bias <= range * q/(1-q)` with the nominal compute
share q. the derivation in specs/fold-mining.md §withholding bias gets that
bound from |D| <= |A|, where A is the set of pool tickets assigned to the
withholder and D the set it withholds, so it is exact only at the realized
share |A|/T. with T = 400 tickets, |A| is binomial(T, q), and the nominal
assertion therefore depends on which pool hemera's hashes draw: whenever the
pool happens to give the withholder more than qT tickets, the true bias can
exceed the nominal bound without contradicting the derivation.

measurement: build-service diagnostic run 40358498794 (jobs 121697288886
pre-port and 121697288138 port), a 32-beacon sweep over q in
{0.05, 0.1, 0.25, 0.5}. pre-port is foculus cdff8dc on hemera 0.3.1 fd55f99;
port is foculus 98df7a6 on hemera 0.4.0 8559b52. command in both jobs:
`cargo test --locked --release --test withholding_sweep -- --nocapture`, with
the sweep file soft3 `release/diag/withholding_sweep.rs` at branch
diag/l4b-withholding 2f1138e.

the nominal bound failed in 12 of 256 runs, and every failure had A/N > q
(N = 400 in every row; bias and both bounds are shares of contributor 0,
the withholder):

```
job 121697288886 (pre-port, hemera 0.3.1)
seed  q     A    A/N    bias      nominal   realized
16    0.05  24   .0600  .000105   .000096   .000117
18    0.05  24   .0600  .000128   .000117   .000142
23    0.05  28   .0700  .000132   .000114   .000163
23    0.10  51   .1275  .000265   .000240   .000316

job 121697288138 (port, hemera 0.4.0)
seed  q     A    A/N    bias      nominal   realized
0     0.05  29   .0725  .000125   .000102   .000152
3     0.05  23   .0575  .000114   .000113   .000130
7     0.05  28   .0700  .000112   .000100   .000143
11    0.05  30   .0750  .000095   .000095   .000146
14    0.05  30   .0750  .000121   .000099   .000153
23    0.05  26   .0650  .000089   .000087   .000115
25    0.05  29   .0725  .000143   .000113   .000168
25    0.10  54   .1350  .000266   .000239   .000336
```

summary, run 40358498794: nominal bound red 4/128 pre-port and 8/128 port;
realized bound `range * A/(N-A)` red 0/256; monotonicity check red 0/256.
the test's own beacon on the port (seed 0, q=0.05) is the clearest case:
A=29 (A/N=0.0725), bias 0.000125 against nominal 0.000102 and realized
0.000152.

fix: `tests/withholding_bias.rs` now asserts against the bound at the realized
share |A|/T, requires 0 < |A| < T, and prints the nominal bound and the
realized share for reference without asserting on them. nonnegativity and
monotonicity in q are unchanged.
