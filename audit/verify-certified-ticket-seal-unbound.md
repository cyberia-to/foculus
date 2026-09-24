# audit: verify_certified_ticket does not bind the seal to its own beacon, cluster or ticket

date: 2026-09-24 · revision: 177fad4c614a8e002aac28c9ca2a8196ce69b6cb (origin/master)

## finding

`src/marginal_cert.rs`'s `verify_certified_ticket` takes `beacon`, `cluster`
and `cert: &CertifiedTicket` (which bundles a `ticket` and a `seal:
FoldSeal`), and runs three checks — the win-test, the marginal replay, and
the seal:

```rust
// HyperNova seal
if !verify_fold_seal(&cert.seal) {
    return false;
}
```

`verify_fold_seal` (`src/ticket_proof.rs`) only checks the seal's own
internal HyperNova consistency (`step::verify_group` plus `step_count ==
steps`); it takes no `beacon`, `cluster` or ticket, so it has nothing to
compare against. Nothing in `verify_certified_ticket` ever recomputes the
statement a seal is supposed to attest to and compares it against
`cert.seal.statement`. That statement is exactly the beacon/cluster binding:

```rust
fn ticket_statement(beacon: &[u8; 32], cluster: &[u8; 32], k: u64) -> Statement {
    ...
    out_buf.extend_from_slice(beacon);
    out_buf.extend_from_slice(cluster);
    let output = *hemera_hash(&out_buf).as_bytes().first_chunk::<32>()...;
    Statement { program_hash: PROGRAM, input_hash: input, output_hash: output, ... }
}
```

`TicketProver::seal` builds exactly this statement from the `beacon`/
`cluster` it is called with, decides a proof against it, and ships both in
the `FoldSeal`. Because `verify_certified_ticket` never recomputes
`ticket_statement` from its own `beacon`/`cluster` arguments and checks it
against `cert.seal.statement`, a seal honestly proven for a completely
different ticket, under a different beacon and cluster, still passes
`verify_fold_seal` when spliced onto an unrelated `CertifiedTicket` — see
the new regression,
`verify_certified_ticket_accepts_a_seal_bound_to_a_different_beacon_cluster`
in `src/marginal_cert.rs`: it certifies two independent winning tickets
under two different `(beacon, cluster)` pairs, replaces the first
certification's `seal` with the second's, and `verify_certified_ticket`
still returns `true` for the spliced result.

This is the same shape row 40 already found three times over (bbg#14's
`verify_particle`, bbg#19's `verify_opening`, bbg#29's `verify_query`): a
verifier that checks a proof's internal self-consistency but never binds it
to the external context — here, the specific beacon, cluster and ticket —
it is presented alongside.

## what a fix would need

`ticket_statement` is already the exact function that encodes the binding;
it is private to `src/ticket_proof.rs`. A fix would export it (or an
equality helper over `Statement`) and have `verify_certified_ticket`
recompute the expected statement from its own `beacon`/`cluster` and the
seal's own `steps` (which for a single-ticket certification is always `1`,
per `certify_ticket`'s `prover.seal(beacon, cluster, 1)`), then assert
`cert.seal.statement == expected` before trusting `verify_fold_seal`'s
result. `Statement` does not currently derive `PartialEq`; adding that (or
comparing field-by-field) is part of the same small change.

It is not attempted here, on the same precedent bbg#14/#19/#29 set: this is
a security-relevant verifier, and a subtly wrong comparison (e.g. comparing
the wrong field, or deriving `k` incorrectly for the batch-seal path used by
`prove_replayed_batch`/`prove_settlement_batch`, which is not
`verify_certified_ticket`'s caller but shares `FoldSeal`) would produce a
check that looks sound against self-consistent fixtures while remaining
unsound. This PR is the audit and the regression, not the fix.

## scope checked

`rg -n 'pub fn verify' src` in this repo, cross-checked against the row-40
PRs already open against it (foculus#27 on `verify_epoch_cert`, foculus#45
on `nmt::verify`): `marginal_cert::verify_certified_ticket` was not yet
covered. Its only production caller path is settlement certification
(`certify_ticket` / `certify_batch`); every call site in this checkout
today is a test, so the gap is not yet reachable from production code, but
it is exactly the check a peer would run on a certified ticket received
over the wire (row 21's gossip, row 34's replication).

## remains

- export `ticket_statement` (or an equivalent equality check) and wire the
  recompute-and-compare into `verify_certified_ticket`, with a second
  reviewer confirming the `k` derivation matches every `FoldSeal` producer
  (`certify_ticket`'s single-ticket `k=1`, `prove_settlement_batch`'s
  `tickets.len()`)
- add `PartialEq` (or a dedicated equality helper) to `zheng::types::Statement`
- once fixed, re-run this PR's regression and confirm it now fails as
  expected
