---
tags: reference, cip
crystal-type: process
crystal-domain: cyber
status: draft
alias: foculus beacon, beacon, epoch beacon
---

# foculus beacon

the epoch randomness beacon $b_E$: a single unbiasable value, fixed after the propose window of epoch $E$ closes, that seeds every settlement ordering and every availability sample. [[foculus]] produces it; [[tru]] and the [[reward specification]] consume it. it is not new cryptography — it is one outer [[VDF]] over entropy the protocol already commits per signal.

## what consumes it

- settlement mining ([[reward specification]] §7): a cluster's Shapley-sample ordering is $\pi(n) = H(b_E \,\|\, \text{cluster} \,\|\, n)$. the un-front-runnability of every ordering reduces to $b_E$ being unpredictable until after propose closes
- data availability sampling ([[structural sync]] P3): the sampled positions are drawn from $b_E$
- any per-epoch randomness a protocol layer needs

## requirements

1. unpredictable until the propose window of epoch $E$ closes, so no contributor places links to exploit a known ordering
2. unbiasable, so no settler or contender can steer it toward favorable orderings
3. verifiable, so any node confirms $b_E$ is the canonical value for $E$ in $O(\log T)$
4. live, so it is defined every epoch, including quiet or partitioned ones

## construction

$b_E$ aggregates the per-signal [[VDF]] proofs of the epoch's finalized, non-conflicting, depth-stable signals, then runs the aggregate through one outer VDF:

$$b_E \;=\; \text{VDF}_{T}\Big(\, H\big(\,\operatorname{sort}\{\, \pi^{\text{vdf}}_i : i \in \mathcal{S}_E \,\}\big)\Big), \qquad \mathcal{S}_E = \{\, \text{signals finalized non-conflicting in } E,\ \text{stable to depth } d \,\}.$$

each $\pi^{\text{vdf}}_i = \text{VDF}(\text{prev}_i, T_{\min})$ is the per-source delay proof a [[signal]] already carries ([[structural sync]] P6). the inner $H(\operatorname{sort}\{\pi^{\text{vdf}}_i\})$ is the epoch's merkle clock restricted to $\mathcal{S}_E$ — a commitment [[foculus]] already maintains over the finalized [[NMT]] root.

when $\mathcal{S}_E$ is empty — a quiet or partitioned epoch — the beacon advances by re-delaying the previous value, $b_E = \text{VDF}_T(b_{E-1})$. the beacon is therefore always defined and always moves.

## why each requirement holds

unpredictable. $\mathcal{S}_E$ is the foculus-finalized set, fixed only after propose closes (a particle is final when $\phi^*_i > \tau$, [[foculus]]). nothing in $b_E$ is known while claims can still be placed.

unbiasable, by three independent barriers:

- aggregation. $b_E$ depends on every signal in $\mathcal{S}_E$; one honest finalized signal makes the inner hash unpredictable, so no single actor controls the aggregate
- per-signal sequentiality. each $\pi^{\text{vdf}}_i$ costs $T_{\min}$ sequential work, so an author cannot cheaply grind its own signal's contribution
- conflict exclusion. [[foculus]] admits one adversarial lever — timing which of two conflicting particles finalizes first. restricting $\mathcal{S}_E$ to non-conflicting signals removes that lever from the beacon entirely

last-includer debiased. the outer $\text{VDF}_T$ has delay $T$ longer than the settle decision window. by the time an actor could evaluate $b_E$ for a candidate last inclusion, the window has closed — the standard VDF debiasing of the last revealer.

verifiable. recompute the inner hash over the finalized [[NMT]] root and verify the outer VDF in $O(\log T)$ ([[VDF]]). $b_E$ is a pure function of the finalized set.

Sybil-resisted by inheritance. only stake-backed signals reach $\tau$ in [[foculus]] (zero-stake gives zero $\phi^*$, so it never finalizes), so the entropy contributors are exactly the stake-weighted finalized set. no separate Sybil defense is needed.

## depth and fork-safety

$\mathcal{S}_E$ is taken stable to finality depth $d$, not at first $\tau$-crossing. this binds the beacon to a graph state past foculus's open threshold-gaming and partition-recovery vectors, and it is the same depth at which the [[reward specification]] escrows its settlement pulse. one parameter $d$ governs both the beacon's stability and the irreversibility of settled rewards.

## parameters

| symbol | meaning | set by |
|---|---|---|
| $T_{\min}$ | per-signal VDF delay (rate limit) | already in [[signal]] / P6 |
| $T$ | outer beacon VDF delay; exceeds the settle decision window, fits inside the epoch | new |
| $d$ | finality depth at which $\mathcal{S}_E$ is taken stable | shared with the reward pulse escrow |

## timeline

```
epoch E:
  propose window open    — neurons gossip claims; nothing of b_E is known
  propose window closes  — claim set frozen
  foculus finalizes      — particles cross τ; conflicts resolve
  depth d passes         — S_E becomes stable
  aggregate              — H(sort{π_vdf_i : i ∈ S_E})   (the finalized merkle clock)
  outer VDF_T            — b_E published, verifiable in O(log T)
  settle window          — settlement mining consumes b_E
```

the outer VDF must finish before the settle window opens and could not have finished before propose closed; that interval is the security margin.

## security residual

$b_E$ inherits foculus's open questions where they touch the finalized set: a successful threshold-gaming or partition-recovery reorg of $\mathcal{S}_E$ would change $b_E$. depth $d$ bounds this — a reorg deeper than $d$ is out of scope, the same assumption the reward pulse escrow makes. the unbiasability argument rests on at least one honest finalized signal per epoch; under a total eclipse of an epoch's finalized set the beacon degrades to $\text{VDF}_T(b_{E-1})$, stale but not adversary-chosen.

## open

- tuning $T$ against the epoch clock and the settle window (epoch length is foculus's, [[provable-consensus]])
- whether the outer VDF is a fresh evaluation or folds into the per-signal VDF chain already present, a possible constant-factor saving
- a formal bound on residual bias from sub-$d$ reorgs, tied to foculus's threshold-gaming analysis

---

see [[foculus]] for finality and the finalized set, [[VDF]] for the delay primitive, [[structural sync]] for the per-signal P6 ordering the beacon aggregates, and the [[reward specification]] for settlement mining, the beacon's main consumer.
