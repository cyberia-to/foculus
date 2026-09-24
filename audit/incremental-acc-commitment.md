---
tags: foculus, rust, rewards, fold-mining, tickets, audit
crystal-type: source
crystal-domain: cyber
---
# incremental acc_commitment

date: 2026-09-24 · revision: 177fad4 (origin/master) · property: launch #5 (fold is a
commutative monoid, decide is O(1))

`acc_commitment` (`src/tickets.rs`) hashed the entire `ClusterAcc.seen`
`BTreeSet<(miner, nonce)>` on every call, so `absorb_ticket` and `fold_acc`
— both of which call it after every mutation — cost `O(k)` per call instead
of the `O(1)` the fold monoid is supposed to have. This was found and
measured, not yet fixed, in a prior unmerged slice on this row: steady-state
`absorb_ticket` at `k=264→265` cost ~656 µs, 61× its cost at `k=0→1`
(~11 µs); `fold_acc` merging two `k=132` accumulators cost ~665 µs, the same
order.

## fix

`ClusterAcc` carries a new field, `seen_digest: [u64; 4]`: an additive
256-bit multiset digest over `seen`, maintained incrementally.

- `elem_digest(miner, nonce)` hashes one `(miner, nonce)` pair to four
  `u64` limbs.
- `absorb_ticket` adds the new element's digest in on insert — `O(1)`.
- `fold_acc`'s disjoint path (`inter == 0`) adds the two digests — `O(1)`.
  Its overlap path subtracts the intersection's digest once
  (inclusion-exclusion: `|A ∪ B| digest = digest(A) + digest(B) -
  digest(A ∩ B)`), so cost stays `O(|intersection|)`, not `O(k)`.
- `acc_commitment` now hashes `seen_digest` (a fixed 32 bytes) instead of
  looping over all of `seen` — the hash call itself is `O(1)` in `k`.
- The two wire-decode sites (`gossip.rs`, `wire.rs`) recompute
  `seen_digest` from the decoded `seen` set once; that path already
  rebuilds `seen` element by element off the wire, so this adds no new
  asymptotic cost there.

Out of scope, left as-is: `fold_acc`'s union of the two `BTreeSet`s
(`out.seen.insert` in a loop) is still `O(k)` — a persistent/mergeable set
structure would remove that too, but that is a larger data-structure change
than this slice. What this closes is specifically the rehash inside
`acc_commitment`.

## measured

```
$ cargo test --release --test incremental_acc_commitment -- --nocapture
[incremental-acc] fold_acc: k=16+16 47917ns, k=128+128 80583ns, ratio=1.68x for 8x the k
[incremental-acc] absorb_ticket avg ns/call: k0..50=74545 k50..550=47649 ratio=0.64x
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

Both ratios sit at or below 1×–1.7×, against a pre-fix baseline of ~61× for
a smaller jump in `k` (0→1 vs 264→265). `fold_acc`'s cost still grows
somewhat with `k` (the `BTreeSet` union above), but nowhere near linearly —
8× the elements does not cost anywhere close to 8× more once the rehash is
gone.

```
$ cargo test --lib
test result: ok. 144 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.86s
$ cargo check --tests
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
$ cargo test
test result: ok. 144 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   (lib)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured   (incremental_acc_commitment)
```

## remains

- the `BTreeSet` union cost in `fold_acc`, noted above.
- the pre-existing approximate overlap handling in `fold_acc`'s sum path
  (`sum_m` double-counts overlapping tickets rather than reconciling
  against raw ticket data) is unrelated and untouched by this slice.
- this checkout's `Cargo.toml` does not resolve independent of row 39's pin
  bump (the same pre-existing gap foculus#35/#42/#47 documented); pins
  were not touched here, `git diff --stat origin/master` for this branch
  touches only `src/tickets.rs`, `src/gossip.rs`, `src/wire.rs`,
  `tests/incremental_acc_commitment.rs`, this file.
