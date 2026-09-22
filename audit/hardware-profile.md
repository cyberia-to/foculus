---
tags: foculus, audit, fold-mining, hardware
date: 2026-09-22
---
# settlement-mining hardware profile

[Property #36](https://github.com/cyberia-to/cyber/blob/master/launch.md) of
the phase-1 launch registry: measure samples/second and joules/sample for a
settlement-mining ticket sample on Apple Silicon, an x86 desktop, a phone
and a GPU, so the per-cluster difficulty schedule (property #4) can equalize
pay per second of work across hardware classes. The 2026-09-22 decisions-log
entry on `launch.md` records the claim to be measured here: a ticket sample
is a tri-kernel recompute over an ε-support (`settlement::marginals`, sparse
matrix-vector passes over Goldilocks, no NTT, no dense matmul) plus a zheng
fold proof; the recompute is memory-latency bound on random graph access,
the proof is bandwidth bound with a hashing tail; unified-memory machines
with fast random access should sit near the optimum and a GPU should gain
little.

This measurement covers the recompute side only — one sample is
`try_settlement_ticket`: an ordering draw plus `settlement::marginals` and
the win-test hash, the same call the settlement-mining loop makes on every
nonce. The proof side (fold/decide/verify wall time) is already measured in
[property #3's audit](ticket-proof-cost.md) once that lands; this file does
not repeat it.

`src/bin/hardware_profile.rs` builds a synthetic ε-support as a ring of
`support` stake links, holds 4 contributors fixed, and grinds
`try_settlement_ticket` with `easy_target()` (always-win, so every attempt
is a genuine sample of the real recompute) for a fixed wall-clock window per
support size. A streaming `sudo -n powermetrics --samplers cpu_power`
sampler (the same technique as `xena/crates/xena-bench/src/power.rs`) gives
mean CPU-domain watts over the same window; joules/sample = mean W ÷
samples/s.

## machine 1 of 4: Apple Silicon (this dev Mac)

Validated with rustc 1.98.0 on macOS 26.4.1 arm64, chip `Apple M4 Max`
(`Mac16,5`), revision `177fad4` (origin/master), after two pre-existing
path-dependency fixes needed to build this checkout at all (see Remains):

- `cargo check --tests`: succeeds.
- `cargo test --release`: 143 lib + 19 test-file tests pass, 0 failed
  (`vdisk::tests::rechunk_on_device_join` is flaky in the full-suite run —
  fails under parallel `cargo test --release`, passes every time run alone
  or when the full suite is re-run; pre-existing, unrelated to this change,
  not touched here).
- `cargo run --release --bin hardware_profile -- --secs 8 --supports 8,64,512,4096`:

  | ε-support (links) | samples/s | mean CPU W | mJ/sample |
  |---|---|---|---|
  | 8 | 672.8 | 5.01 | 7.4 |
  | 64 | 123.6 | 6.41 | 51.8 |
  | 512 | 17.2 | 5.11 | 297.7 |
  | 4096 | 2.1 | 4.99 | 2370.7 |

  idle CPU-domain baseline (`sudo -n powermetrics -i 500 --samplers
  cpu_power`, first sample dropped as streamer warm-up): 0.03–0.08W, so the
  measured watts above are essentially all workload draw on this
  single-threaded loop — the wattage is flat across support sizes (as
  expected: one P-core busy-looping the whole time) and joules/sample
  scales with samples/s alone.

## reading

samples/s falls roughly with support size (8→64 is 8x support for 5.4x
fewer samples/s; 64→512 is 8x support for 7.2x fewer; 512→4096 is 8x
support for 8.2x fewer) — close to linear in ε-support size, consistent
with a sparse recompute whose cost is dominated by the O(support) graph
build and power-iteration passes rather than a fixed per-sample overhead.
No NTT or dense matmul shows up as expected: there is no super-linear
blowup as support grows 512x.

This is one hardware point of four. It says nothing yet about how an x86
desktop, a phone, or a GPU compare — the claim in the decisions log
(unified-memory Apple Silicon near the optimum, GPU gains little) is
unverified until at least one of those runs. What it does establish: the
benchmark itself (`hardware_profile` binary, `--supports` sweep) is
portable — the same binary run on other hardware produces a comparable
row, so the remaining three points are a `cargo run --release --bin
hardware_profile` invocation each, not new code.

## remains

- x86 desktop, phone, GPU runs — this worker only has the one Apple
  Silicon dev Mac; the binary is ready to run on the other three
- reconcile with property #3's proof-side cost (`ticket-proof-cost.md`,
  currently in an unmerged PR) into one per-sample total once both are on
  the same base revision
- the two pre-existing dependency-path fixes this measurement needed to
  build at all — `Cargo.toml` pinned `bbg = "0.2"`, `zheng = "0.3"`, and
  `tape = { package = "cyber-tape", path = "../tape/impl/rust" }`, none of
  which resolve against the current sibling checkouts (`bbg` is at 0.3.0,
  `zheng` at 0.4.0, and the `tape` repo itself was renamed to `tade`); this
  PR carries the fix so this benchmark and its own tests build, matching
  the same fix already carried independently by the other open `foculus`
  launch PRs (#6–#13), which is why they will collide on this file once
  one of them merges
- a multi-threaded sample rate (the mining loop runs many workers in
  parallel in production); this measures one worker's serial throughput
  only, the unit the per-cluster difficulty schedule prices
