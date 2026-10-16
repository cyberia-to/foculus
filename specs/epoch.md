---
tags: reference, cip
crystal-type: process
crystal-domain: cyber
status: draft
alias: foculus epoch, epoch clock, epoch boundary
---

# foculus epoch

the epoch boundary $B_E$: the one object per epoch that every honest [[neuron]] derives identically from gossip alone, and from which the [[foculus beacon|beacon]], settlement ([[reward specification]] §6–§7), [[fold-mining]], [[tok]] and the [[bbg]] archive read their clock. [[foculus]] produces it; nothing downstream defines it. this page exists because until 2026-09-27 every consumer assumed the boundary and no mechanism produced it — evidence in `audit/epoch-clock-gap-2026-09-27.md`.

## what a boundary is

$$B_E \;=\; \big(E,\; R_E,\; \mathcal{S}_E\big)$$

- $E$ — the epoch index, a monotone integer, $B_{E}$ valid only after $B_{E-1}$ is final;
- $R_E$ — the merkle-clock root ([[structural sync]] layer 2) at which epoch $E$'s propose window closes; the claim set of $E$ is exactly the claims whose signals are $\le R_E$ in the merkle-clock partial order;
- $\mathcal{S}_E$ — the finalized, non-conflicting signals of epoch $E$, i.e. those $\le R_E$ that have crossed $\tau$ and are stable to depth $d$ ([[foculus beacon]] § construction). $\mathcal{S}_E$ is a function of $R_E$ and the finality set $F$, so agreeing on $R_E$ is agreeing on $\mathcal{S}_E$.

the unit of $d$ is fixed here, once: **epochs**. "stable to depth $d$" means $B_{E+d}$ is final. every other page that uses $d$ in blocks or tip height ([[light-money]], `src/live.rs`) is to be read in this unit.

## requirements

in the register of [[foculus beacon]] § requirements:

1. **agreement** — honest neurons converge on one $B_E$ per $E$; two candidate boundaries for the same $E$ are a conflict and resolve by the same rule as every other conflict;
2. **liveness** — a boundary exists every epoch, including quiet and partitioned ones; the beacon's empty-set rule ($b_E = \text{VDF}_T(b_{E-1})$) already covers the quiet case and inherits from here;
3. **unforgeable cadence** — nobody advances or delays $E$ cheaper than the protocol's own cost of time: the per-signal $T_{\min}$ [[VDF]] and the beacon's outer $\text{VDF}_T$ are the only clocks, no wall time;
4. **read-only downstream** — beacon, settlement, fold, tok, bbg archive and the annuity consume $B_E$; none may define, advance or reinterpret it. the `live.rs:529` state copy between two nodes is the anti-test of this page.

## the mechanism (draft, candidate a — foculus-native)

the boundary is a [[particle]], finalized like any other.

1. **proposal.** any neuron may emit a boundary signal $\beta = (E, R, \text{prev} = b_{E-1})$ where $R$ is its current merkle-clock root. the signal is rate-limited by its own $T_{\min}$ VDF like every signal, and is valid only if it carries $b_{E-1}$, the previous beacon — so a proposal for $E$ cannot exist before epoch $E-1$'s beacon is published. this is the cadence: one outer VDF per epoch, unforgeable by construction of the beacon.
2. **conflict.** two boundary signals for the same $E$ conflict (resource collision on the resource "epoch $E$", [[foculus]] § conflict definition, type three). exclusive support applies: each neuron links to at most one candidate, the honest strategy is first-seen, support switching resolves near-symmetric splits.
3. **finality.** the candidate whose $\phi^*$ crosses $\tau$ within its $\varepsilon$-support domain is $B_E$; the others are pruned. finality latency is the ordinary foculus latency, $R_{\text{safe}}\Delta \approx 7$ s in the worked example of [[foculus security at scale]].
4. **the cut.** $\mathcal{S}_E$ = finalized non-conflicting signals $\le R_E$. a signal that reaches finality after $B_E$ is final but is $\le R_E$ belongs to $\mathcal{S}_E$ only if it finalizes before $B_{E+d}$; otherwise it belongs to the epoch in which it finalizes. this is what "stable to depth $d$" buys: after $B_{E+d}$, $\mathcal{S}_E$ never changes.
5. **the beacon.** $b_E = \text{VDF}_T(H(\operatorname{sort}\{\pi^{\text{vdf}}_i : i \in \mathcal{S}_E\}))$ exactly as [[foculus beacon]] specifies; the only change is that $\mathcal{S}_E$ is now defined.

why this may be enough: it uses no machinery foculus does not already specify — signals, conflicts, exclusive support, $\phi^*$ finality, the beacon VDF chain. the epoch clock becomes foculus's own product rather than a new consensus layer.

## alternatives kept on the page

- **(b) ticket DAG.** settlement tickets ([[reward specification]] §7) are proof-of-work objects; give them parents and a $k$-cluster ordering (GHOSTDAG) and read blue score as the network tick, $E = \lfloor \text{blue}/N \rfloor$, $\mathcal{S}_E$ = signals referenced by blue tickets, $d$ in blue-score units. gives $d$ a physical unit and names the "block producer" of [[structural sync]] (the ticket miner). costs a second security assumption (honest majority of ticket work, per-cluster cost heterogeneity, `launch.md` row 4) and must keep cyberlinks out of blocks.
- **(c) hybrid.** (a) for agreement, with the boundary signal committing to the blue-set root of (b), so $\mathcal{S}_E$ carries both $\phi^*$ finality and PoW cost-of-time.

the order of testing is (a) first: a network gate in which two nodes finalize $B_E$ from gossip with no state copy. if it holds, (b) and (c) are not needed for phase 1.

## open

- circularity: `proposals/view-certificates.md` fixes $\tau$ per epoch from $b_E$; the boundary for $E$ must finalize under $\tau$ derived from $b_{E-1}$, never $b_E$. to be stated there.
- who proposes: every neuron may; the cost is one signal's $T_{\min}$. whether a proposal should also require stake is open.
- $R_E$ under partition: two partitions each finalize a candidate only if each holds enough certified $\phi^*$ mass ([[vec|VEC P2]]); the safety argument of [[foculus]] § safety proof applies unchanged, the liveness argument needs the partition case written out.
- pipelining: [[life of a signal]] pipelines $E+1$'s propose during $E$'s settle; under this page $E+1$'s proposal needs $b_E$, so propose for $E+1$ opens when $b_E$ is published, not when $E$ settles. the walkthrough is to be redrawn.
- the epoch *length* is not a parameter of this page: it is the outer VDF delay $T$ plus finality latency; [[foculus parameters]] lists $T$ as undetermined and that remains the one knob.

## contradictions this page retires once it lands

listed in `audit/epoch-clock-gap-2026-09-27.md` §2.4: total order vs no ordering; "block producer" vs no proposer; "no rollbacks" vs depth $d$ (to be scoped: particles are final at $\tau$, *settlement* binds at depth $d$); the unit of $d$; block-time targets; "same pre-epoch bbg root" vs "the header it observed" (the boundary $R_E$ is the pre-epoch root all neurons share); "VDF epoch".
