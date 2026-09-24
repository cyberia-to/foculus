# audit: verify_receipt does not bind fold_seal to its own epoch/beacon (fold_seal half of a known gap)

date: 2026-09-24 · revision: 177fad4c614a8e002aac28c9ca2a8196ce69b6cb (origin/master)

## finding

foculus#52 (row 40, open) documents that `src/rewards.rs`'s `verify_receipt`
checks `SettleReceipt.ticket_seal` and `SettleReceipt.fold_seal` the same
way — `verify_fold_seal(seal)` alone, which only checks a seal's own
internal HyperNova consistency and has no `epoch`/`beacon`/`claims_root` to
compare against — and regression-tests the gap for `ticket_seal`. Its
"scope checked" section explains why `fold_seal` was left untested: "only
`ticket_seal` is exercised in the new regression, since `settle_epoch_tickets`'s
test setup does not reliably produce a `fold_seal` (`prove_fold_tree` falls
back to the unsealed monoid path ... `fold_seal` is `None` whenever that
fallback triggers)."

That turns out not to hold for the simplest single-miner case:
`settle_epoch_tickets` with one claim, one miner, `TicketPolicy { want: 4,
max_attempts: 64, .. }` — the exact fixture #52's own `ticket_seal`
regression already uses — reliably produces `fold_seal: Some(_)` too
(verified deterministic over repeated runs; `prove_fold_tree`'s single-leaf
path, `src/ticket_proof.rs:174-181`, only needs one `ClusterAcc`, which
`self_fold` always yields). So the identical splice #52 proved for
`ticket_seal` reproduces for `fold_seal` with the same fixture shape: two
independent settlements under two different epochs (different beacons,
different claims_roots) each produce a valid, internally-sound `fold_seal`;
swapping the first receipt's `fold_seal` for the second's, while leaving its
own `epoch`/`beacon`/`shares` (and hence `receipt_hash`) untouched, still
verifies. New regression:
`verify_receipt_accepts_a_fold_seal_from_a_different_epoch`
(`src/rewards.rs`), sibling to #52's
`verify_receipt_accepts_a_ticket_seal_from_a_different_epoch`.

## what a fix would need

Same as #52's own writeup: recompute the statement `fold_seal` is supposed
to attest to (the fold-tree equivalent of `ticket_statement(beacon, cluster,
k)`, `k` = leaf count) from `receipt.beacon`/`receipt.claims_root`, and
compare it against `fold_seal.statement` before trusting
`verify_fold_seal`'s answer — the same change #52 already scoped for
`ticket_seal`, applied to the same function's `fold_seal` branch. Not
attempted here, on the same precedent as bbg#14/#19/#29, #51 and #52: this
is the reward-payout artifact itself, and a subtly wrong `k` derivation
would produce a check that looks sound against self-consistent fixtures
while remaining unsound.

## scope checked

This closes #52's own named gap (no `fold_seal` regression existed) rather
than finding a new call site; #52's fix, once written, should make this
repo's `fold_seal` branch and `ticket_seal` branch symmetric and both
regressions should then fail as expected against the fixed code.

## remains

- the actual fix: export the statement-recompute for both seal fields and
  wire it into `verify_receipt`, per #51's and #52's fix sketches
- `verify_live_receipt` (`src/epoch.rs`) calls `verify_fold_seal` on
  `receipt.ticket_seal`/`receipt.fold_seal` a second time, independently of
  `verify_receipt` — once `verify_receipt` itself binds both seals, confirm
  whether `verify_live_receipt`'s redundant checks become provably
  subsumed or need the same treatment
