# audit: verify_receipt does not bind ticket_seal/fold_seal to the receipt's epoch, beacon or claims_root

date: 2026-09-24 · revision: 177fad4c614a8e002aac28c9ca2a8196ce69b6cb (origin/master)

## finding

`src/rewards.rs`'s `SettleReceipt` carries two optional HyperNova seals,
`ticket_seal` and `fold_seal`, alongside `epoch`, `beacon`, `claims_root` and
`receipt_hash`. `receipt_hash` is documented and computed as "Hemera over
(epoch ‖ beacon ‖ sorted (neuron, amount))" (`receipt_hash`,
`src/rewards.rs:486`) — it does not cover either seal. `verify_receipt`
checks the three pieces independently:

```rust
pub fn verify_receipt(receipt: &SettleReceipt) -> bool {
    if receipt.receipt_hash != receipt_hash(receipt.epoch, &receipt.beacon, &receipt.shares) {
        return false;
    }
    if let Some(art) = &receipt.beacon_artifact {
        if !verify_beacon(art) || art.beacon != receipt.beacon {
            return false;
        }
    }
    if let Some(seal) = &receipt.ticket_seal {
        if !verify_fold_seal(seal) { return false; }
    }
    if let Some(seal) = &receipt.fold_seal {
        if !verify_fold_seal(seal) { return false; }
    }
    true
}
```

`verify_fold_seal` (`src/ticket_proof.rs`) only checks a seal's own internal
HyperNova consistency; it takes no `epoch`, `beacon` or `claims_root` and so
has nothing to compare against. `beacon_artifact`'s check cross-references
`receipt.beacon` explicitly (`art.beacon != receipt.beacon`) — the same
pattern `ticket_seal`/`fold_seal` are missing.

`settle_with_peer_accs` (`src/rewards.rs:333`) shows what a seal is actually
proven for: `ticket_seal` comes from
`marginal_cert::prove_replayed_batch(base, &contribs, ctx, params, &b_e, &cr,
&tickets)` — bound to this epoch's beacon `b_e` and claims_root `cr` via the
same `ticket_statement(beacon, cluster, k)` mechanism documented in
`audit/verify-certified-ticket-seal-unbound.md` (this repo, row 40, a
sibling finding on `marginal_cert::verify_certified_ticket`). `fold_seal`
comes from `prove_fold_tree(&b_e, &cr, &leaves)`, the same binding. Because
`verify_receipt` never recomputes either seal's expected statement from its
own `receipt.epoch`/`receipt.beacon`/`receipt.claims_root` and compares it,
a `ticket_seal` (or `fold_seal`) honestly proven for one epoch's settlement
still verifies once spliced onto a different `SettleReceipt` — including one
with a different epoch, beacon and claims_root — as long as the receipt's
own `epoch`/`beacon`/`shares` (and hence `receipt_hash`) are left alone.

New regression `verify_receipt_accepts_a_ticket_seal_from_a_different_epoch`
(`src/rewards.rs`) proves it: two independent settlements are run under two
different epochs (different beacons, different claims_roots), the first
receipt's `ticket_seal` is replaced with the second's, and `verify_receipt`
still returns `true` for the spliced result.

This is the same root cause as `marginal_cert::verify_certified_ticket`
(audited separately, same row) surfacing at a second call site:
`verify_fold_seal` alone never binds a `FoldSeal` to the beacon/cluster
context its caller believes it is checking. `SettleReceipt` is the actual
reward-payout artifact — the wallet mints from `verify_receipt`'s answer
(its own doc comment: "wallet mints from this, not free-form amounts") — so
this instance is reachable from the payout path itself, not only from
ticket-level certification.

## what a fix would need

Same shape as the sibling audit: export (or re-derive) the statement each
seal is supposed to attest to — `ticket_statement(beacon, cluster, k)` for
`ticket_seal` (k = number of tickets folded), the fold-tree equivalent for
`fold_seal` — and have `verify_receipt` recompute it from
`receipt.beacon`/`receipt.claims_root` and compare against
`seal.statement` before trusting `verify_fold_seal`'s answer, mirroring the
`beacon_artifact` check already present two lines above. Not attempted
here, on the same precedent as bbg#14/#19/#29 and this row's sibling PR: a
subtly wrong `k` derivation (`ticket_seal`'s tickets count vs `fold_seal`'s
leaf count) would produce a check that looks sound against self-consistent
fixtures while remaining unsound.

## scope checked

Both `Option` seal fields on `SettleReceipt` (`ticket_seal`, `fold_seal`)
share the identical check shape and the identical gap; only `ticket_seal`
is exercised in the new regression, since `settle_epoch_tickets`'s test
setup does not reliably produce a `fold_seal` (`prove_fold_tree` falls back
to the unsealed monoid path per `settle_with_peer_accs`'s own comment,
"Prefer HyperNova-proven fold tree; fall back to monoid assemble" —
`fold_seal` is `None` whenever that fallback triggers). The finding and fix
sketch apply identically to `fold_seal` once a test setup reliably takes
the proven path.

## remains

- export the statement-recompute for both `ticket_seal` and `fold_seal`,
  wire it into `verify_receipt` alongside the existing `beacon_artifact`
  cross-check, with a second reviewer confirming both `k` derivations
- add a second regression once a `fold_seal`-producing test setup exists,
  covering the identical gap on that field
- coordinate with `marginal_cert::verify_certified_ticket`'s fix (this
  row's sibling PR) since both need the same `Statement` equality primitive
