---
tags: foculus, audit, zheng, proofs, settlement
date: 2026-10-09
---
# sound settlement proofs — inventory, statements, measurements

Work package C of the [proof-system repair](https://github.com/cyberia-to/soft3/blob/docs/proof-repair-frontier/proposals/proof-system-repair.md):
move foculus off zheng's legacy `commit / fold / decide / verify`, whose fold
the verifier never checked (zheng `specs/decider.md`; attack tests in zheng
`rs/src/folding/fold.rs`), onto zheng's public certificate v3.

Stand: `~/cyber/.stands/C`. foculus base 9e32aa6 (`fix/shared-storage-dependencies`),
zheng 0c882e7 (`feat/public-certificate-v3`, zheng#46), nox 2f09ca3
(`release/0.4`), lens f214ae0, tru 4cec250, tok e27da0d, bbg/hemera/strata as
checked out in the stand. Rust 1.95.0, `--release`, Apple M4 Max.

## 1. inventory — every zheng call before this change

| site | zheng API | supposed to establish | what it established |
|---|---|---|---|
| `step.rs` | `ccs::universal_ccs/universal_witness`, `verify` over a `TraceProof` | shared fold step: one add row whose registers hash the public material | knowledge of *some* satisfying add row; the material was bound only through the `Statement` the decider absorbed (its own doc said so) |
| `ticket_proof.rs` `TicketProver`, `prove_settlement_batch`, `prove_fold_tree`, `verify_fold_seal` | `fold`, `decide`, `Accumulator`, `Transcript` | σ of a batch of settlement tickets: each `m(n)` is the correct v★ marginal under π(n) at b_E, the win test holds | nothing about marginals; the accumulator's fold was never checked by the verifier (decider.md), any satisfying witness verifies for any statement |
| `ticket_proof.rs` fold tree | same | σ_f: each fold-tree node is `fold(acc_L, acc_R)` | same — nothing |
| `marginal_cert.rs` `certify_ticket`, `prove_replayed_batch` | via `TicketProver` | "σ certifies Δφ⁺/Shapley marginals" | the replay (native recomputation) was sound; the seal added nothing |
| `rewards.rs`, `epoch.rs` receipts (`ticket_seal`, `fold_seal`) | `verify_fold_seal` | tok may mint against the receipt | `verify_receipt` checked seals with no public inputs — sums and shares were the settler's word; peer self-accumulator sums were folded in unchecked |
| `pay_proof.rs` `prove_pay` / `verify_pay` | `fold`, `decide` over one add row | conservation: the pay intent's legs sum to `total_out` | verify re-derived the accumulator from the statement and compared — it checked that the prover ran the same code on the same three numbers; no leg was ever seen |
| `tip.rs` `TipProver`, `Tip::join_checkpoint`, `advance_fold`, `redecide` | `fold`, `decide`, `verify` | light client: the checkpoint root is the result of the folded history | the statement was derived from the checkpoint itself; a root the fold never saw was accepted (test `join_cannot_detect_a_checkpoint_root_the_fold_never_saw`) |
| `epoch_cert.rs` `verify_phi_on_cert` | `phi::verify_phi_star` (internally `verify_spmv` → legacy `verify`) | φ* is the tri-kernel fixed point of the committed graph | sound by its final native recomputation; legacy SpMV proofs were dead weight; `graph_commit` never compared with the graphs passed in (launch row 40) |
| `epoch_cert.rs` `verify_epoch_cert(cert, None)` | `verify_fold_seal` | settle receipt valid | returned true for a settling cert without any settle input |
| `chain.rs` `Signal.proof: Option<zheng::Proof>`, `signal_codec/proof.rs` | `zheng::Proof` wire | σ on pay signals, durable | a Brakedown/HyperNova proof body: carried, never sound |
| `cli.rs`, `node.rs`, `radio_settle.rs`, `gossip.rs`, `wire.rs` | none directly; reach the above through `settle_with_peer_accs`, `ClusterAcc` | — | — |
| `finality_evidence.rs` | doc pointer to `prove_phi_star` | — | — |
| tests: `tests/signal_codec.rs` | `zheng::Proof`, `SumcheckPoly` | codec round trip of the legacy body | — |

## 2. statements and how each is proven now

Rule (`src/proof/mod.rs`): a statement splits into (a) statement checks the
verifier recomputes in every profile — hashes, orderings, win tests, chain
links over public data; (b) a witness relation, a CCS the verifier builds from
public data only, proven by `Proof::Public` = zheng certificate v3 bound to a
statement digest; (c) witness checks no CCS expresses yet, run natively on the
disclosed witness — each one a named gap for the succinct profile. `Proof` is
one enum; a succinct or accumulated profile is a new variant behind the same
`prove` / `verify`.

| statement | (a) recomputed | (b) relation, v3 certificate | (c) native witness check | API |
|---|---|---|---|---|
| ticket: `m(n)` is the v★ marginal vector under π(n) at b_E for cluster C, and `H(b_E‖C‖n‖id‖commit(m)) < target` | π(n) from (b_E, n); `commit(m)`; score and `score < target`; shape | `m[π_j] = v_j − v_{j−1}`, `v_{−1}=0`; m pinned public, coalition values v_j free | `v_j = impulse(base, π_0..π_j).directed` | `ticket_proof::{prove_ticket, verify_ticket}` |
| cluster settlement: `(Σm, k, commitment)` is the sum over a set of valid, distinct tickets | duplicates by (id, n); the monoid rebuilt from accepted tickets (`absorb_tickets`, one commitment hash) | every ticket's proof (linear) | — | `cluster::{verify_cluster, accept_tickets}` |
| peer self-accumulator | every named (id, n) redrawn from public data and proven; claimed sums must equal the redrawn ones or the peer is dropped | per ticket | per ticket | `cluster::redraw_peer` |
| fold-lottery step σ_f | `acc' = fold(acc_L, acc_R)`, pair-id, fold score < target | — | — | `cluster::verify_fold_ticket` |
| settle receipt | claims_root, Δφ⁺ of the claim set, shares from the verified accumulator (or the Monte-Carlo samples), receipt hash, target = protocol target | via the cluster | via the tickets | `settle_tickets::verify_settlement` |
| epoch certificate | hash, tip grade, beacon VDF, claims root; fails closed when a bound part lacks inputs | via the receipt | — | `verify_epoch_cert`, `verify_epoch_cert_with_phi` |
| φ* | graph commitment over (T, S, degree, teleport, weights), φ₀ hash, `iterations` tri-kernel steps, φ* hash | — (a v3 certificate of the SpMV rows checks the same products at more bytes) | — | `phi::{compute_phi_star, verify_phi_claim}` |
| pay σ: the signal's legs sum to `total_out` as integers | `content_id`, leg count, total from the signal | each leg and running total packed into 62 boolean bits, `s_j = s_{j−1} + a_j`, `s_{L−1} = total` (pinned) — no wrap mod p | legs equal the signal's link amounts | `pay_proof::{prove_pay, verify_pay}` (take the signal) |
| tip: the root is the last step of the chain a trusted head commits to | `head_i = H(head_{i−1}‖height‖root‖leaves)`, heights strictly increasing | — | — | `tip::{TipChain, Tip::join_chain, advance_chain}` |
| signal proof field | codec v2: profile byte, statement, canonical certificate values; v1 decoded only without a proof | — | — | `signal_codec` |

`tests/no_legacy_zheng.rs` fails if any `zheng::…` path in `src` is outside the
allow-list (`execution::Certificate`, `execution::certificate`, CCS types, phi
graph types); the φ* differential test may call `prove_phi_star`.

## 3. what cannot be made better than this without the later packages

Plainly:

- Nothing here is succinct or zero-knowledge. The public profile discloses the
  witness and costs at least what recomputation costs. For tickets the
  dominant cost is the native `impulse` check (c), not the certificate: the v3
  row check is 0.001–0.02 ms of a 0.25–810 ms ticket verify (table 5.1).
- Ticket marginals: no CCS or nox relation for `tru::impulse` exists. Until one
  does, every profile — succinct included — must keep the native check, and
  verifying a ticket stays one impulse per contributor.
- Cluster: verification is linear in tickets. "512 tickets decide in one proof"
  (phase-3 gate) needs a step relation for (b)+(c) under ARC accumulation.
- σ_f is a recomputation; whether it becomes an ARC step is open (proposal §8).
- Tip: the chain proves only that the root is the chain's last step. That each
  root is a valid transition from the one before needs a bbg transition
  relation and accumulation (phase 3). Light join is linear: one hemera per step.
- Pay: legs are disclosed and bound to `content_id` natively; hiding them needs
  the zk profile and hemera-over-bytes in-circuit.
- φ*: recomputation; a succinct φ* needs the tri-kernel as an accumulated step
  relation.

## 4. zheng APIs needed and not available

1. `zheng::phi::trikernel_step` is `pub fn` inside the private `trikernel`
   module and not re-exported; foculus carries a port pinned by a differential
   test. Export it (and a non-panicking `spmv_native`, which indexes edges
   unchecked).
2. `certificate::verify` returns `()`; there is no way to get the assembled
   witness `z` it checked. foculus re-assembles and re-checks the rows itself so
   its native checks read the same vector. A `verify` returning `CCSWitness`
   (or a pub `assemble`) removes that duplication.
3. No CCS or nox program for `tru::impulse` (owned by tru; listed for
   completeness — it is the gap in (c)).
4. No relation for hemera over bytes. The nox structural hash (op 15) hashes
   nouns: costs measured below make an in-circuit win test or content binding
   unattractive in the public profile.
5. `ExecutionStatement::relation` / `bindings` are `pub(crate)`; a caller that
   wants `certificate::verify` over a nox program's relation with extra pins
   cannot build them. Not needed here; noted for callers mixing both.

## 5. measurements

`cargo run --release --example settlement_bench`, 2026-10-09, Apple M4 Max.
Wire bytes = profile 1 + statement 32 + count 4 + 8 per free value.

### 5.1 ticket proof (one ticket)

base: a ring of `particles` with chords (2 links per particle); contributors
one link each.

| contributors n | base links | free values | wire bytes | prove ms | verify ms | v3 rows only ms |
|---|---|---|---|---|---|---|
| 1 | 6 | 1 | 45 | 0.258 | 0.255 | 0.0012 |
| 3 (tests) | 6 | 3 | 61 | 0.731 | 0.725 | 0.0017 |
| 8 | 128 | 8 | 101 | 25.529 | 24.589 | 0.0054 |
| 16 | 512 | 16 | 165 | 191.014 | 189.804 | 0.0092 |
| 64 | 512 | 64 | 549 | 810.350 | 811.954 | 0.0109 |

### 5.2 cluster settlement verify (strict: every ticket)

| tickets | n | base links | verify ms | per ticket ms | proof bytes |
|---|---|---|---|---|---|
| 8 | 3 | 6 | 5.28 | 0.661 | 488 |
| 64 | 3 | 6 | 46.79 | 0.731 | 3,904 |
| 512 | 3 | 6 | 354.29 | 0.692 | 31,232 |
| 8 | 8 | 128 | 206.87 | 25.859 | 808 |
| 64 | 8 | 128 | 1,834.97 | 28.671 | 6,464 |
| 512 | 8 | 128 | 14,363.76 | 28.054 | 51,712 |

Before sealing the cluster commitment once, 512 tickets at n=3 took 792.76 ms
(1.548 ms per ticket): `absorb_ticket` rehashed the whole seen-set on each
insert. `absorb_tickets` fixed it (table above).

### 5.3 pay proofs

| legs | free values | wire bytes | prove ms | verify ms |
|---|---|---|---|---|
| 1 | 84 | 709 | 0.078 | 0.050 |
| 2 | 211 | 1,725 | 0.101 | 0.079 |
| 16 | 1,978 | 15,861 | 0.611 | 0.450 |
| 64 | 8,028 | 64,261 | 2.263 | 1.697 |

### 5.4 tip chain (light join recomputes every step)

| steps | join ms | per step µs |
|---|---|---|
| 1,000 | 14.02 | 14.02 |
| 100,000 | 1,391.04 | 13.91 |

### 5.5 probe: an in-circuit win test via nox `certify_execution`

Program `[15 [0 1]]` (structural hash of the subject) over k input atoms
(subject has k+1 atoms), v3 certificate, same machine; raw bytes = 8 × free
values. Not committed (probe only).

| k | free values | raw bytes | prove ms | verify ms |
|---|---|---|---|---|
| 1 | 1,248 | 9,984 | 6.76 | 5.89 |
| 4 | 3,120 | 24,960 | 14.57 | 14.88 |
| 8 | 5,616 | 44,928 | 27.43 | 25.05 |
| 17 | 11,232 | 89,856 | 51.75 | 49.08 |
| 32 | 20,592 | 164,736 | 96.80 | 92.63 |

The win test hashes ~17 field atoms (beacon, cluster, nonce, identity,
commitment): ~90 KB and ~50 ms per ticket to prove one hash the verifier
recomputes natively in microseconds, and the result would still be a different
hash (nox structural hash ≠ hemera over bytes). Statement-only hashes stay
native in every profile.

## 6. tests

`cargo test --release`: 230 passed, 0 failed (baseline at 9e32aa6: 207).
lib 165, attack_vectors 17, legacy_frames_strict 6, million 1,
no_legacy_zheng 2, signal_codec 8, stress 12, structural_sync 19.

New, by requirement: honest ticket verifies; wrong marginal (cannot be proven;
a telescoping forgery fails the witness check), marginals of another ordering,
wrong beacon, wrong identity (score, then statement binding), a certificate of
another ticket, another base graph, losing / malformed tickets — all rejected;
cluster of 8 with one bad ticket rejected strictly and dropped leniently;
duplicates; lying peer accumulator dropped; fold step tamper; pay proof moved
to another signal, tampered certificate, reshuffled legs with the same total,
wrapping legs, oversized amounts — all rejected; codec v2 round trip, v1 without
proof decodes, v1 with a legacy proof rejected at its position, profile and
residue and trailing-zero and count limits; tip chain tamper, height
regression, untrusted head; φ* differential against zheng, graph / teleport /
φ₀ / result / iteration binding, malformed graphs; receipt tamper (shares,
dropped ticket, Δφ⁺, claims, target), Monte-Carlo receipt recomputed; the
legacy-path guard and its own scanner test.

`cargo clippy --release --all-targets`: no warning in a touched file.
`--features net` does not build at the base either: ed25519-dalek
3.0.0-pre.1 fails against its pkcs8 (third-party); `node.rs` and
`radio_settle.rs` use only signatures this change keeps
(`settle_with_peer_accs`, `ClusterAcc`, `verify_receipt`).

## 7. downstream and open

- Breaking for cyb (`crates/cyb`, `core`): `prove_pay(&signal)` /
  `verify_pay(&proof, &signal)`; `TipProver::push_height`, `TipTrust::ChainVerified`,
  `Tip::join_chain`; receipts carry `settlement` instead of `ticket_seal` /
  `fold_seal`. cybergraph `tests/native_storage.rs` uses the old `prove_pay`.
- bbg `Checkpoint.acc` is still the legacy `zheng::Accumulator`; foculus
  ignores it and writes `None`. Its removal belongs to bbg.
- `fold_score` does not bind the fold miner's identity, so a winning fold
  ticket is claimable by anyone who sees it; the spec's "same ticket structure"
  suggests it should. Not changed here.
- `fold_acc` double-counts sums of overlapping inputs; settlement no longer
  trusts it.
- `LiveNode` and the tests still use `easy_target()`; the receipt verifier
  compares the proof's target with the caller's protocol target.
- foculus needs zheng with `execution::certificate` (zheng#46) on its sibling
  path; origin `fix/shared-storage-dependencies` alone does not provide it.
