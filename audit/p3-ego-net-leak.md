---
tags: foculus, audit, privacy, security
date: 2026-09-19
---
# P3 audit — settlement tickets publish the full per-contributor marginal vector

Property 12 (`cyber/launch.md`): "P3 — settling a marginal m(n) reveals
nothing of the miner's ego-net beyond public aggregates." Checked against
`cyberia-to/foculus@177fad4c614a8e002aac28c9ca2a8196ce69b6cb` (default
branch `master`) with `rustc 1.98.0`.

## finding

the current settlement-ticket design does not satisfy P3. it is not a bug
in an otherwise-private scheme — the certification path is designed to be
fully public at the per-contributor level, by a different, load-bearing
requirement ("anyone with the public claim set can verify without trusting
the miner", `src/marginal_cert.rs:18`).

- `SettlementTicket` carries the raw marginal vector in the clear:
  `pub marginals: Vec<Fx>` (`src/tickets.rs:49`), one field per
  contributor, produced by `settlement::marginals(base, contribs, order,
  ctx, params)` under the beacon-seeded ordering π(n)
  (`src/tickets.rs:165`, `src/marginal_cert.rs:56`). this vector is exactly
  the miner's Shapley sample: which contributors moved value and by how
  much, under one evaluation order.
- `commit_marginals` (`src/tickets.rs:120`) hashes that same plaintext
  vector; the ticket's `commitment` field is a commitment to public data
  the ticket already carries, not a hiding commitment — nothing about the
  vector is withheld from a party holding the ticket.
- `verify_settlement_ticket` (`src/tickets.rs:217`) only checks the
  commitment and the win-test score against a ticket the caller already
  has in hand; it does not gate access to `ticket.marginals`, which is a
  public struct field.
- `replay_marginals` (`src/marginal_cert.rs:43`) goes further: it
  recomputes the full marginal vector from the public `(base, contribs,
  beacon, nonce)` and requires an exact element-wise match against
  `ticket.marginals` (`src/marginal_cert.rs:61`) before certifying. the
  module doc states the design intent directly: "anyone with the public
  claim set can verify without trusting the miner"
  (`src/marginal_cert.rs:18`).
- the HyperNova seal in `ticket_proof.rs` does not change this: `fold_settlement`
  folds `beacon ‖ cluster ‖ nonce ‖ miner ‖ commitment` — the commitment,
  never the marginals themselves (`src/ticket_proof.rs:56-62`). the proof
  layer already treats the marginal vector as public input, consistent
  with `replay_marginals` needing it in the clear.

so every party that certifies or folds a settlement ticket today sees the
full per-contributor marginal vector for that miner's one evaluation
order — not a public aggregate. `ClusterAcc.sum_m` (`src/tickets.rs:70-101`)
is the only aggregate-shaped value in the module, and it is a running sum
over accepted tickets, not a replacement for the per-ticket disclosure:
each ticket that feeds it was already public before folding.

## why this is not a quick fix

replay-based certification is what currently gives the swarm "no trusted
miner" verification (component table: foculus "single machine, never a
network"). making `m(n)` private requires the certifier to check the same
facts — correct ordering, correct marginal computation, correct commitment
— without seeing the vector: a zero-knowledge statement over
`settlement::marginals`, sealed by a real proof rather than a replay. this
is exactly the gap the component table already names: zheng "execution
proofs bind computation to result; fold + decide exist; private execution
incomplete" (`cyber/launch.md` component table, zheng row). closing P3
is downstream of that work, not a foculus-local fix.

## what this closes and what remains

this audit gives property 12 a precise, code-cited statement of the gap
in place of an unexamined "open". it does not change behavior and closes
nothing by itself. remains, in order:

1. specify the zero-knowledge statement `replay_marginals` would need to
   check without material disclosure (public inputs: `base`, `contribs`,
   `beacon`, `nonce`, `commitment`; private witness: the marginal vector).
2. land private execution in zheng (already tracked, not P3-specific).
3. replace `replay_marginals`'s exact-match check with proof verification;
   `SettlementTicket.marginals` stops being a field anyone but the
   producing miner holds.
4. re-audit `ClusterAcc`/fold-mining for any other point for a full
   per-ticket vector crosses a trust boundary in the clear.

## see also

- [[foculus]] fold-mining spec: `specs/fold-mining.md`
- component table, [[zheng]] row: `cyber/launch.md`
- property 12, `cyber/launch.md` property registry
