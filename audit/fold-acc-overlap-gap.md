---
tags: foculus, audit, fold-mining, cyber
crystal-type: reference
crystal-domain: cyber
---
# fold_acc overlap gap

date: 2026-09-23 · revision: 177fad4 (origin/master) · property #5

`specs/fold-mining.md` states the fold accumulator is "a commutative monoid
under fold: the order of accumulation does not affect the result" and "the
monoid counts each canonical (id, n) pair once," with a re-submitted nonce
"detected at fold time and discarded."

`src/tickets.rs::fold_acc` detects overlap (`left.seen ∩ right.seen`) but
does not discard it: the overlap branch's own comment says "prefer left's
sums when fully overlapping," yet the code below re-sums `left.sum_m[i] +
right.sum_m[i]` unconditionally, identical to the disjoint branch. `k` is
correctly recomputed from `out.seen.len()`, but `sum_m` is not, so
`fold_acc(x, x)` returns `sum_m = 2·x.sum_m` with `k = x.k` unchanged —
`mean_shares` then returns double the true per-neuron mean.
Any fold-tree assembly that folds two accumulators sharing even one ticket
corrupts the Shapley-share estimate `rewards.rs` mints against, silently
and without error.

## measurement

`fold_acc_overlap_doubles_the_mean_a_monoid_law_violation`
(`src/tickets.rs`) folds a self-accumulator with itself and asserts the
existing `fold_acc` reproduces exactly this doubling — a live
demonstration of the bug on real ticket data (3 winning settlement
tickets, 2 contributors), not a synthetic case.

## fix

`fold_acc_checked` is added alongside `fold_acc` (unchanged, so no
existing caller is affected): it computes the same seen-intersection check
and, on overlap, returns `Err(FoldOverlap::NonDisjoint(n))` instead of
mis-summing. A compact `ClusterAcc` retains no raw per-ticket marginals,
so there is no way to *compute* the correct sum on overlap — surfacing the
overlap to the caller is the only sound move. In honest fold-tree
assembly, accumulators folded together come from disjoint leaves, so
`Ok` is the expected path; `NonDisjoint` signals a replayed ticket or a
broken tree construction and should abort the fold step rather than mint
against a corrupted mean.

`fold_acc_checked_matches_fold_acc_when_disjoint` confirms the two
functions agree exactly whenever there is no overlap, so switching a
caller from `fold_acc` to `fold_acc_checked` is a drop-in change on the
honest path.

## remains

`try_fold_ticket`, `grind_fold`, and `assemble_fold_tree` still call
`fold_acc` (unchecked). Wiring them to `fold_acc_checked` — and deciding
what a fold miner does on `Err` (skip the pair, slash, or fall back to a
different pair) — is the next slice; it touches the fold-mining lottery's
public signatures and is out of scope here.
