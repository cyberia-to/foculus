# audit: fold-mining accumulators cross the wire with the full marginal vector in the clear

date: 2026-09-23 · revision: 177fad4 (origin/master) · repo: foculus

## scope

Property #12 — P3, a miner's ego-net stays hidden beyond public aggregates —
was audited against `SettlementTicket.marginals` and `replay_marginals`
(`audit/p3-ego-net-leak.md`, not yet merged; this file stands independently
of it). That audit's remains named a fourth, unclaimed step: re-audit
`ClusterAcc` and the fold-mining path for other plaintext crossings beyond
the settlement ticket itself. This is that pass, over `src/tickets.rs`,
`src/gossip.rs`, `src/wire.rs` and `src/radio_settle.rs`.

## finding

`ClusterAcc` (`src/tickets.rs:70-78`) holds `sum_m: Vec<Fx>`, the running,
un-normalized sum of every sample's per-contributor marginal vector — the
same shape `SettlementTicket.marginals` leaks, but earlier in the pipeline
and before any settlement proof exists.

Two independent wire encoders serialize `sum_m` field-for-field, in the
clear:

- `encode_acc_body` (`src/wire.rs:199-210`) and its matching
  `decode_acc_body` (`src/wire.rs:213-234`).
- `encode_self_acc` (`src/gossip.rs:94-111`) and its matching
  `decode_self_acc` (`src/gossip.rs:114-164`), tagged `b"FSC1"`.

`radio_settle::Settler::publish_self_acc` (`src/radio_settle.rs:358-366`)
wraps a `ClusterAcc` in `SettleMsg::SelfAcc` and hands it to
`self.radio.publish(...)` — a real network send, not a local-only call.
The receiving side, `peer_accs` (`src/radio_settle.rs:378-382`) and
`collect_self_accs` (`src/radio_settle.rs:231-249`), reconstructs the
same `ClusterAcc`, `sum_m` intact, from whatever peers publish.

So every miner in a cluster gossips its running per-contributor marginal
sum to every other miner in that cluster, once per self-fold round, well
before a cluster's tickets are settled or folded into the O(1)
accumulator. This is a second, earlier P3 crossing: the ticket-level leak
(row 12's existing finding) exposes the vector once, at settlement; this
one exposes a live-updating version of the same vector continuously,
peer-to-peer, during mining.

`rewards.rs:295` (`settle_with_peer_accs`, `peer_accs: &[ClusterAcc]`) and
`rewards.rs:361` (`root.mean_shares(&neurons)`) are the consumers —
confirming the plaintext vector is load-bearing for reward computation
today, not incidental.

## verification

```
$ grep -n "sum_m" src/tickets.rs src/wire.rs src/gossip.rs src/radio_settle.rs src/rewards.rs
```
reproduces every citation above against this revision.

```
$ cargo check --tests
```
docs-only change, no production code touched by this audit.

## remains

- item 2 and item 3 of the original P3 remains (land zheng private
  execution; rewrite `replay_marginals` to verify a proof instead of
  comparing plaintext) close this crossing too if the fix folds `sum_m`
  behind the same private-execution boundary rather than patching gossip
  and settlement separately.
- a fix here needs `ClusterAcc` to carry a commitment to `sum_m` (it
  already carries `commitment: [u8; 32]`, unclear today whether that
  commits to `sum_m` or only to `seen`/`k` — worth its own follow-up
  read) plus a proof of correct accumulation, rather than the vector
  itself, on the wire.
- not yet checked: `assemble_fold_tree` and the cluster-tree fold above
  self-fold (`src/tickets.rs`, `src/rewards.rs`) for whether the same
  plaintext vector propagates up the tree unchanged, or is aggregated
  into a form that no longer names individual contributors.
