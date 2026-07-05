---
tags: reference, cip
crystal-type: process
crystal-domain: cyber
status: draft
alias: fold mining, fold lottery, second lottery, settlement aggregation, fold accumulator
---

# fold mining

settlement mining ([[reward specification]] §7) produces a swarm of winning tickets. each ticket is a [[zheng]] proof that one Shapley marginal $m(n)$ was correctly evaluated under a [[foculus beacon|beacon]]-seeded ordering. the swarm average converges to the fair division by [[Hoeffding's inequality|Hoeffding]]. but before [[tok]] can mint, those tickets must aggregate into one $O(1)$ object — or a flood of minimum-cost tickets makes settlement verification an asymmetric-cost DoS.

fold mining is the second lottery that does this aggregation. every fold step is real proof-of-useful-work: it collapses two [[zheng]] proof instances into one accumulator using [[recursion|HyperNova IVC]]. like settlement mining fuses securing the chain with computing fair division, fold mining fuses accumulation with the stakeless onramp — same [[Goldilocks field processor|GFP]] primitives, same ticket structure, same progress-free lottery, same karma- and stake-blind entry. it is the natural completion of the first lottery, not a separate protocol.

## the settlement estimator — the tru boundary

before the fold aggregates tickets, the tickets must be *drawn*. this is the first lottery, and it sits on a clean architectural seam. the seam is drawn by *reasons for change*: what changes when the intelligence layer changes belongs to [[tru]]; what changes when the reward *mechanism* changes belongs here.

tru owns exactly one thing — the **physical magnitude**, `Δφ⁺`, a pure function of the graph. its whole interface to the reward is `tru::impulse(base, batch).directed`: the directed focus shift a set of links produces. that is the value oracle, and it is *all* tru contributes.

foculus owns the **cooperative game and its solution** — because they are reward policy, not intelligence. the surprise-weighted value `v★(S) = Δφ⁺(A^eff ∪ ρ·S)`, the per-contributor marginal, the exact Shapley division, and the lottery are `foculus::settlement`. changing the fair-division rule (Shapley → Banzhaf → proportional) touches foculus, never tru. a settlement miner:

```
v★(S) = tru::impulse(base, ρ-weighted links of S).directed   # the oracle (tru)
π(n)  = ordering(b_E ‖ cluster ‖ n)                          # beacon-seeded (foculus)
m(n)  = marginals(base, contribs, π(n))    # v★(prefix∪{i}) − v★(prefix)  (foculus)
win   iff  H(b_E ‖ cluster ‖ n ‖ id(ν) ‖ commit(m(n))) < target
```

each winning `(n, m(n), σ)` is a ticket; the swarm mean of `m(n)` converges by [[Hoeffding's inequality|Hoeffding]] to the exact Shapley division `shapley_exact` defines by full enumeration. the value, the game, the division, the beacon, the ordering, the win-test, and the aggregation below are all foculus; conservation and the mint are [[tok]]. **the entire interface to tru is one call — `impulse`** — with no reward concept (surprise, coalition, Shapley) leaking across it. that single call is the versioned contract to hold stable.

## the fold step

the primitive is [[accumulator|HyperNova IVC folding]]:

$$\text{fold}(\text{acc},\, \text{instance}) \;\to\; \text{acc}' \quad \approx 30\ \mathbb{F}_p\ \text{ops}\ +\ 1\ \text{Hemera hash.}$$

the accumulated relation for a cluster is the triple $\bigl(\sum_n m(n),\, k,\, \pi_{\text{acc}}\bigr)$ — running sum of accepted marginals, sample count, validity accumulator. this is a commutative monoid under fold: the order of accumulation does not affect the result, so the tree assembles in any topology without coordination, preserving the leaderless property. once assembled, the root decider costs $\sim 825$ constraints ($\sim 10$–$50\ \mu$s verify). one constant-size object proves every marginal sample in the cluster.

duplicate nonces are excluded structurally: the settlement win-test commits to $\bigl(\mathrm{id}(\nu),\, n\bigr)$ jointly, so no two tickets share a nonce under different identities. a re-submission of an existing nonce is a collision on the committed value, detected at fold time and discarded. the monoid counts each canonical $(\mathrm{id},\, n)$ pair once.

## self-fold first

before broadcasting, each settlement miner folds its own batch of winning tickets into a single self-accumulator. the cluster fold tree then reaches depth $O(\log k_{\text{miners}})$ rather than $O(\log k_{\text{tickets}})$; each tree node is one miner's pre-aggregated batch, already proven. self-folding can begin as soon as a miner's first winning ticket arrives — it runs concurrently with the settle window and need not wait for it to close.

## the fold lottery

each internal fold step is a lottery ticket with the same structure as a settlement ticket. a fold miner:

1. selects an input pair $(\text{acc}_L,\, \text{acc}_R)$ — two existing accumulators at the same tree level;
2. computes $\text{acc}' = \text{fold}(\text{acc}_L,\, \text{acc}_R)$ and its validity proof $\sigma_f$;
3. holds a winning fold ticket iff
$$H\!\big(b_E \;\|\; \text{cluster} \;\|\; \texttt{"fold"} \;\|\; \text{level} \;\|\; \text{pair-id}(L, R) \;\|\; f \;\|\; \text{acc}'.commitment\big) \;<\; \text{fold-target},$$
and publishes $\bigl(f,\, \text{acc}_L,\, \text{acc}_R,\, \text{acc}',\, \sigma_f\bigr)$ to claim the step.

`level` and `pair-id` bind the ticket to a specific tree position, preventing a miner from re-using a fold result at the wrong level. binding `acc'.commitment` to the hash makes the ticket valid only for this fold output, so a miner cannot grind hashes and submit a self-favoring input pair after learning the result — the same preimage discipline as settlement mining.

progress-freedom holds per fold step: each nonce $f$ is an independent Poisson trial against fold-target. the first valid submission wins and earns the step's subsidy with no latency advantage beyond having a valid input pair to fold. the second lottery is exactly as leaderless as the first.

## completion and liveness

### the precision target

[[Hoeffding's inequality|Hoeffding]] gives the minimum sample count for a $\delta$-confidence estimate accurate to $\pm\varepsilon$:

$$k_{\min} \;=\; \left\lceil\frac{\ln(2/\delta)}{2\varepsilon^2}\right\rceil.$$

once the cluster accumulator's sample count $k \ge k_{\min}$, the Shapley estimate meets the precision guarantee. fold work earns nothing beyond this point: fold-grinding cannot run as a secondary puzzle.

### deadline and default

the fold tree must reach its root before the fold deadline $t_{\text{fold}}$, a fixed offset from the epoch boundary sized to fit inside the epoch and leave margin for [[tok]] execution.

if $k < k_{\min}$ at $t_{\text{fold}}$ — too few settlement samples arrived — the cluster does not stall the chain. the settled fraction is applied as the pulse for that epoch; the remaining unminted balance defers to the annuity stream ([[reward specification]] §11–§12). [[tok]] receives a valid (possibly partial) input regardless of cluster coverage.

if $k \ge k_{\min}$ but the fold tree has not reached a root by $t_{\text{fold}}$ — enough samples exist but fold work stalled — the deepest available sub-accumulator is used as the cluster's settlement. the sample mean over the accumulated subtree still satisfies Hoeffding (it is a valid sub-sample); the unpublished remainder falls to the annuity. fold stalls are bounded by the Poisson analysis: an honest majority mining population completes the tree well before the deadline under normal conditions.

### tok gating

[[tok]] does not execute a cluster's settlement until:

1. the root accumulator (or the deadline's default sub-accumulator) is published, and
2. the root decider proof passes in $O(1)$.

one verified proof per cluster, no earlier.

## difficulty

`fold-target` adjusts per epoch so the tree over $k_{\text{miners}}$ self-accumulators converges to a root within the fold window. difficulty is ratio-policed against the settlement difficulty: thin settlement work → fewer input pairs → fold-target loosens so the tree still completes. a fold step exercises the same four GFP primitives (fma, ntt, p2r, lut) as a marginal sample, so difficulty has a physical floor set by hardware throughput, not a synthetic puzzle.

## payment

fold steps are paid as tier-2 settlement subsidy from the per-epoch security budget ([[reward specification]] §8). a winning fold ticket earns from the same pool as a winning settlement ticket. the allocation is:

- capped at the Hoeffding precision target: once $k \ge k_{\min}$ samples are folded into the accumulator, fold rewards turn off for that cluster;
- proportional to the sample mass folded in the winning step — a step that folds a large subtree earns more than a step over two single tickets;
- karma-blind and stake-blind, like the settlement subsidy: a new [[neuron]] with zero $CYB$ participates in fold mining on the same terms as settlement mining.

## epoch lifecycle

fold mining occupies the third phase of the epoch, after the propose and settle windows:

```
propose window      — neurons gossip claims; b_E unknown
propose closes      — claim set frozen
foculus finalizes   — particles cross τ; S_E stable at depth d
beacon b_E          — outer VDF_T completes; settlement miners consume b_E
settle window       — settlement mining: each hash → one Shapley sample
self-fold (concurrent) — miners fold their own batches as tickets arrive
fold window         — fold lottery: tree assembles to root (or default at t_fold)
tok execution       — decider proof checked; conservation clip applied; mint executes
```

self-fold begins as soon as a miner's first winning ticket exists, not after the settle window closes. in practice the two windows overlap substantially, and the root accumulator may be available long before $t_{\text{fold}}$.

## settlement liveness (closes §15 of [[reward specification]])

[[reward specification]] §15 names settlement liveness as unspecified, listing three missing pieces. this document closes all three:

| open item (§15) | answer |
|---|---|
| minimum sample count $k_{\min}$ | Hoeffding target: $\lceil \ln(2/\delta) / 2\varepsilon^2 \rceil$, with $\varepsilon$, $\delta$ protocol constants |
| fold deadline $t_{\text{fold}}$ | epoch-boundary + fold-window (protocol constant, sized to leave margin for tok) |
| default when too few samples | pulse on settled fraction; remainder to annuity; no chain stall |
| default when fold stalls | deepest available sub-accumulator; decider still $O(1)$; remainder to annuity |
| tok gating condition | root-or-default decider proof passes before mint executes |

the fork-safety requirement from §15 — "settlement must bind to a $d$-deep stable state with the pulse escrowed until that depth" — is satisfied by the beacon construction ([[foculus beacon]]), which already restricts $\mathcal{S}_E$ to signals stable to depth $d$. the pulse escrow and the beacon stability share the same parameter $d$.

## open

- formal bound on fold-stall probability as a function of the mining population's honesty majority and fold-window length
- whether fold-target should be a hard cutoff at $k_{\min}$ or a smoothly decaying function of marginal Hoeffding gain (step function is simpler; decay lets late fold steps earn something for precision beyond $k_{\min}$)
- whether the outer VDF in the beacon can reuse the fold accumulator as a commitment substrate, saving a separate commitment step
- exact ratio policy for fold-target vs settlement-target under sustained thin-work conditions

---

see [[foculus beacon]] for the epoch randomness beacon $b_E$ the fold lottery consumes. see [[accumulator]] and [[recursion]] for the HyperNova fold primitive and cost numbers. see [[reward specification]] §7–§8 for settlement mining mechanics and the economics that the fold lottery extends. see [[foculus]] for epoch finality and the full seven-step protocol. see [[tok]] for conservation clip and mint execution.
