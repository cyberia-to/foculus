---
tags: foculus, spec, privacy, zheng, tickets
crystal-type: article
crystal-domain: cyber
date: 2026-09-23
---

# the marginal-hiding statement for settlement tickets

property 12 (`cyber/launch.md`): "P3 — settling a marginal m(n) reveals nothing of the miner's ego-net beyond public aggregates." `audit/p3-ego-net-leak.md` traced the gap: `replay_marginals` (`src/marginal_cert.rs:43`) certifies a [[settlement ticket]] by recomputing the full per-contributor marginal vector from public data and requiring an exact match against `ticket.marginals`, a public struct field. this is remains item 1 from that audit: the [[zero-knowledge]] statement `replay_marginals` needs once the marginal vector stops being disclosed.

this is a specification only. no code in this repository changes; it fixes the target shape for the [[zheng]] private-execution backend (component table: "execution proofs bind computation to result; fold + decide exist; private execution incomplete") that remains item 2 depends on, and for the `replay_marginals` rewrite that is remains item 3.

## the statement

public input, exactly the parameters `replay_marginals` already takes minus the vector it currently trusts in the clear:

- `base: &[Link]` — the graph the marginals are computed against (public per [[P1|P1 signal privacy]]: the edge is public, the author is private)
- `contribs: &[Contribution]` — this epoch's ρ-weighted contribution list (public: it is exactly what today's plaintext `ticket.marginals` is computed from)
- `beacon: [u8; 32]`, `nonce: u64` — the epoch beacon and the miner's chosen nonce, both already public (`SettlementTicket.nonce` is a public field)
- `commitment: [u8; 32]` — `commit_marginals(m)`, already a public field of `SettlementTicket` today

private witness:

- `m: Vec<Fx>` — the marginal vector, one entry per contributor, currently `SettlementTicket.marginals`

relation the statement proves, mirroring `replay_marginals`'s own three checks (`src/marginal_cert.rs:43-64`) with the vector moved from a public equality check to a witness:

1. `order = settlement::ordering(contribs.len(), &beacon, nonce)` — a pure function of public inputs, so the verifier derives it exactly as `replay_marginals` does today, in-circuit, no witness needed for this step
2. `m' = settlement::marginals(base, contribs, &order, ctx, params)` — the circuit recomputes the reference marginal vector from public inputs the same way `settlement::marginals` does natively; this is the one step that needs zheng's execution backend, because `value()` (the Δφ⁺ evaluation `settlement::marginals` calls per prefix) is the same tri-kernel-adjacent computation [[provable consensus circuit specification|provable consensus]] already scopes for in-circuit SpMV
3. `m == m'` element-wise (the witness must equal the honestly recomputed vector — this is what makes the statement a *replay* proof and not merely "some vector hashes to the commitment")
4. `commit_marginals(m) == commitment` — binds the witness to the public commitment already carried by the ticket

what changes downstream of this spec, not decided here: `SettlementTicket.marginals` stops being a `pub` field anyone holding the ticket can read; a `zheng` proof object replaces it, and `certify_ticket`/`replay_marginals` become "verify the proof" instead of "recompute and compare." that rewrite is remains item 3 and is out of scope until item 2 (a zheng backend that can execute `settlement::marginals`, not just check a flat program against a public input/output pair) exists.

## why the witness is the vector and not the order

the order `π(n)` is a deterministic function of `beacon` and `nonce`, both already public fields of the ticket — hiding it would require hiding `nonce` itself, which `settle_score` (`src/tickets.rs:131`) also consumes as a public input to the win-test. nothing in property 12 asks for the ordering to be private: the leak the audit names is the *per-contributor* breakdown, not which order the miner picked among n! possibilities. an attacker who only sees `order` and `commitment` — no `m` — recovers no more than they already get from the public claim set today.

## why `contribs` stays public

`contribs` (who submitted a claim, with which links, at what ρ) is the same public input `settle_epoch`/`settle_epoch_tickets` already consume from `claims` (`rewards.rs`); nothing about property 12 asks for the *contributor set* to be hidden, only the *marginal amount* each contributor is shown to have moved under this miner's evaluation. conflating the two would fold property 12 into P1 (rows 10, 29), which the audit already separates: P1 is about author privacy on the edge itself, P3 is about not exposing a Shapley breakdown over an otherwise-public set.

## see also

- `audit/p3-ego-net-leak.md` — the finding this statement closes remains item 1 of
- [[fold-mining]] — the settlement-ticket protocol `SettlementTicket` and `replay_marginals` belong to
- [[provable consensus circuit specification]] — the in-circuit SpMV cost model the marginal recomputation step (relation step 2) will need to fit inside
- `zheng/specs/execution.md` — the existing public-execution statement shape this spec's public/private split mirrors
