# the epoch boundary is not a consensus object — audit of the missing clock

Date: 2026-09-27. Scope: foculus specs + `src/`, soft3 node, bbg, cybergraph,
tru/specs, cyber wiki, `cyber/launch.md`. Three read-only sweeps with
file:line evidence; no code changed. Companion market/design reads that
triggered this: `warriors/audit/parano1d-vs-uhash-2026-09-27.md` §3 (blue
score as the shared clock), `warriors/audit/zkas-2026-09-27.md` §5.

## 1. the finding in one paragraph

Every economic and consensus mechanism downstream of foculus consumes an
epoch index $E$ and an epoch boundary — the beacon ($\mathcal{S}_E$ "fixed
after propose closes"), settlement depth $d$, the settle and fold windows,
$t_{\text{fold}}$, the equivocation predicate (`P_a.epoch = P_b.epoch`),
tok's per-epoch mint budget, the annuity's per-epoch accrual, bbg's
per-epoch archive and pruning. No mechanism produces that boundary. In
code the propose window closes when a unit test calls `freeze()`; two
nodes "agree" on the boundary and on $\mathcal{S}_E$ by one test copying
the other's fields; the beacon's input set is every VDF the node has ever
ingested, with no finality, conflict or depth filter; block height is a
per-process mutex counter. The specs defer the epoch clock to each other
in a circle and disagree on its unit (100 blocks, 1000 blocks, $E$ blocks,
85 s). No document lists "who decides the epoch boundary" as open.

## 2. evidence

### 2.1 code (foculus)

| where | what it shows |
|---|---|
| `src/epoch.rs:90-103` `EpochRunner::freeze` | "Close propose window" is a caller-invoked method; only precondition: claims non-empty |
| `src/epoch.rs:245-252` `next_epoch` | epoch id is a plain `u64 + 1`; requires phase `Settled` — epochs are strictly sequential in code, while `docs/explanation/life-of-a-signal.md:57,65` pipelines E+1 during E |
| `src/live.rs:212-289` `close_and_settle_epoch` | freeze → beacon → settle → cert → next; called only from unit tests (`src/live.rs:476-589`); not used by `cli.rs`, `node.rs`, `bin/main.rs` |
| `src/live.rs:529-540` | the only two-node test: "Bob mirrors claim set + beacon (network agreement)" — `bob.epoch.beacon = alice.epoch.beacon.clone(); bob.epoch.claims_root = alice.epoch.claims_root` |
| `src/live.rs:226,392` | beacon input = `self.signals.values().map(|s| s.vdf.clone())` — every signal ever ingested, never cleared between epochs |
| `src/beacon.rs:79-86` `collect_signal_outputs` | the only filter is `vdf::verify`; no finality, conflict or depth check; `:90` and `src/epoch.rs:333` call the set "depth-stable" / "Simulated" in comments only |
| `src/beacon.rs:118-138` `verify_beacon` | checks the VDF and the hash over the artifact's own `signal_root`; never that `signal_root` equals any finalized set, nor that `prev` is the verifier's previous beacon |
| `src/epoch_cert.rs:120-187` `verify_epoch_cert` | internal consistency only; no $\mathcal{S}_E$ check, no chain-of-prev |
| `src/gossip.rs:27`, `src/wire.rs:282` | settle topic = `claims_root`: peers share a topic only if they already froze identical claims |
| `src/live.rs:73,108,401,429` | `settle_depth: 2` gates reward maturity against `tip.height`, a local counter bumped once per ingested signal |
| `src/rewards.rs:223,305`, `src/cli.rs:643,1037,1068` | CLI and settle-net paths pass an empty signal set and `GENESIS_PREV`; epoch is a user argument |
| `src/radio_settle.rs:276-296` | peer ticket collection closes on `accs.len() >= want` or a 5 s wall-clock timeout; no $t_{\text{fold}}$, no $k_{\min}$ gate (`tickets.rs:105-116` used only in a test) |
| `src/conflict.rs:13,40-45` | doc says equivocation = same author + epoch + signal; implemented key = `hemera(neuron ‖ step)` — the epoch field is not in the key |
| `src/epoch.rs:144`, `src/radio_settle.rs:331` | "coordinator folds" / "coordinator collects peer accs" |

### 2.2 code (soft3 node, bbg, cybergraph)

| where | what it shows |
|---|---|
| `cybergraph/src/native/mod.rs:74-81` `Block` | `{height, timestamp, supply, weight, root, signal}` — no prev hash, no parent set, no epoch, no beacon |
| `soft3/crate/src/node/routes.rs:6-9`, `node.rs:196-209` | block order = order requests acquire `Arc<Mutex<Node>>`; one accepted signal → one block (`routes.rs:112-115`, `native-node.md:18`) |
| `bbg/rs/src/transition/mod.rs:215-231`, `state.rs:14` | `height += 1` per signal; `EPOCH_BLOCKS = 100` is a pruning cadence only |
| `bbg/rs/src/checkpoint.rs:12-31` | `Checkpoint {root, acc, height}` — no prev link; `bbg/docs/explanation/architecture-overview.md:77`: "no sync committee, no header chain". The "BBG header" of `tru/specs/rewards.md:135` has no type |
| grep `vdf\|causal\|total_order\|tiebreak` in bbg, cybergraph, soft3 | comments only; the "causal → VDF → hash" total order of `bbg/docs/explanation/signal-sync.md:21,87` is not implemented anywhere; `bbg/specs/sync.md:26` disowns it |
| `foculus/src/fork.rs:1-60` | the only fork-choice code (`Serialize`, `MinHash`, `Focus`); unused by soft3, cybergraph, bbg |

### 2.3 specs: the circular deferral

- `foculus/specs/beacon.md:85` — open: "tuning $T$ against the epoch clock … (epoch length is foculus's, [[provable-consensus]])".
- `foculus/specs/provable-consensus.md:185` — "practical for epoch boundaries (e.g. every 100 blocks = 500 seconds)". An example, not a rule.
- `foculus/specs/parameters.md:41-42,52,55-56,79` — window lengths, $T_{\min}$, $T_{\text{beacon}}$ and $d$ "explicitly undetermined"; "$d$ is the single highest-leverage undetermined parameter"; treated as values to pick, not as a missing mechanism.
- `foculus/docs/explanation/latency-targets.md:20` and `soft3/roadmap/component-boundaries.md:38` — foculus *owns* "epoch schedule, beacon delay, settlement depth $d$". Ownership assigned, mechanism absent.
- Epoch length by document: ~100 blocks (`cyber/epoch.md:9`, `cyber/hierarchy.md:230`), 1000 blocks (`cyber/research/bbg.md:248`, `cyber/research/structural-sync.md:214`), $E$ blocks (`bbg/specs/temporal.md:104`), 85 s / ~25 s wall time (`life-of-a-signal.md:99`, `latency-targets.md:199`).

### 2.4 specs: contradictions that the missing clock explains

| claim A | claim B |
|---|---|
| `vec.md:143`, `structural-sync.md:193-200`: deterministic total order from data alone | `protocol.md:90`, `overview.md:57`: no ordering, it emerges from $\phi^*$; `foculus-vs-crdt.md:67`: VDF order is partial |
| `structural-sync.md:58,312`: a "block producer" assigns $t$ | `protocol.md:90`, `life-of-a-signal.md:15`: no proposer, no block producer |
| `protocol.md:161`: "no rollbacks, no probabilistic confirmation depth" | `beacon.md:54,81`, `rewards.md:343`, `parameters.md:52`: reorgs up to depth $d$ |
| $d$ in epochs (`latency-targets.md:95`, `life-of-a-signal.md:120`) | $d$ in tip height (`light-money.md:144`, `src/live.rs:429`) |
| `rewards.md:219`: difficulty holds block time | `life-of-a-signal.md:72`: difficulty is not tuned to block time |
| `whitepaper.md:918`: all neurons use the same pre-epoch `bbg_root_t` | `rewards.md:135`: each proposes "against the BBG header it observed" |
| `structural-sync.md:198`: "same VDF epoch" tiebreak | "VDF epoch" defined nowhere |

### 2.5 what consumes the boundary (so the blast radius is clear)

`tru/specs/rewards.md:141,147,149,173,282-294,343`; `foculus/specs/beacon.md:11-34,54,62,66-81`; `fold-mining.md:72-76,101-128`; `protocol.md:58,207`; `tok/rs/src/mint.rs:58` ("this epoch's emission"); `bbg/specs/pruning.md:83`, `privacy.md:190-195`, `neuron-state.md:80`; `cyber/specs/money-loop.md:64,246`; `cyber/3c.md:69-74`, `cyber/research/oikos.md:64` (cross-book matching "in the same foculus epoch"). `cyber/launch.md` rows 6, 8, 9, 35 assume an epoch exists; no row asks for one.

## 3. the problem, stated as a requirement

Foculus must produce, without a leader and without wall-clock agreement, a
sequence of **epoch boundaries** $B_1, B_2, \dots$ such that every honest
node derives the same triple

$$(E,\; \mathcal{S}_E,\; \text{root}_E)$$

— the index, the finalized non-conflicting signal set the beacon hashes,
and a commitment to it — and such that $\mathcal{S}_E$ is stable to depth
$d$ in a unit that is defined once. Downstream contracts (beacon,
settlement, fold, tok, bbg archive, annuity) then read the boundary; none
of them may define it. Two independent nodes must agree on $E$ and
$\mathcal{S}_E$ from gossip alone — the `live.rs:529` state copy is the
anti-test.

Three properties are required of the mechanism, in the same register as
`beacon.md` § requirements:

1. **agreement**: honest nodes converge on one $(E, \mathcal{S}_E)$ per $E$;
   conflicting boundaries resolve like any other conflict;
2. **liveness**: a boundary exists every epoch, including quiet and
   partitioned ones (the beacon already handles the empty case);
3. **unforgeable cadence**: nobody can advance or delay $E$ cheaper than
   the protocol's own cost of time — the beacon's outer VDF and the
   per-signal $T_{\min}$ are the existing sources.

## 4. candidate mechanisms (not decided here)

**(a) foculus-native: the boundary is a particle.** A node proposes
"epoch $E$ closes at merkle-clock root $R$" as a signal; competing
proposals for the same $E$ form a conflict group (`protocol.md` §
exclusive support); the one whose $\phi^*$ crosses $\tau$ first is the
boundary; $\mathcal{S}_E$ = the finalized set below $R$. Uses only
machinery foculus already specifies. Costs: one extra $\phi^*$ finality per
epoch (≈ $R_{\text{safe}}\Delta$ ≈ 7 s per `security-at-scale.md:178`);
needs a rate limit on boundary proposals (the beacon VDF chain gives one:
a proposal for $E$ is valid only with $b_{E-1}$); possible circularity if
$\tau$ is fixed per epoch from $b_E$ (`proposals/view-certificates.md:73`).

**(b) PoW-native: a DAG of settlement tickets.** Tickets already are
proof-of-work objects with a lottery (`rewards.md` §7, `src/tickets.rs`).
Give each ticket parents, order them with a $k$-cluster rule (GHOSTDAG),
and read blue score as the network tick: $E = \lfloor \text{blue}/N \rfloor$,
$\mathcal{S}_E$ = signals referenced by blue tickets, $d$ in blue-score
units. Resolves the "block producer" contradiction (the producer is the
ticket miner) and gives $d$ a unit. Costs: a second security assumption
(honest majority of ticket work, with per-cluster cost heterogeneity that
`launch.md` row 4 is still banding); the knowledge lane must stay out of
the blocks.

**(c) hybrid.** (a) for agreement, with the boundary particle committing
to the blue-set root of (b) so that $\mathcal{S}_E$ inherits PoW
cost-of-time and $\phi^*$ finality at once.

The launch hypothesis to test first is (a): if foculus can finalize a
boundary particle across two nodes from gossip alone, the epoch clock is
foculus's own product and no new consensus layer is needed.

## 5. where the formulation should live

- requirement + mechanism: a new page `foculus/specs/epoch.md` ("the epoch
  clock"), referenced by `beacon.md`, `fold-mining.md`, `parameters.md`
  (which then lists $d$'s unit as *defined* there) and `protocol.md`
  (which must retract "no rollbacks" or scope it to particles, not
  settlement);
- launch gate: one new row in `cyber/launch.md` § property registry —
  "epoch boundary is a consensus object: two nodes agree on $E$ and
  $\mathcal{S}_E$ from gossip alone, no state copy" — owner foculus,
  evidence = a network test replacing `live.rs:529-540`;
- economics: `tru/specs/rewards.md` §15 gains one line that settlement
  *reads* the boundary from foculus and does not define it;
- the contradictions in §2.4 are fixed by the spec landing, not before.
