---
tags: reference, cip
crystal-type: process
crystal-domain: cyber
status: draft
---

# foculus consensus specification

the [[collective focus theorem]] proves that token-weighted [[random walk]] on a strongly connected [[cybergraph]] converges to a unique $\phi^*$. foculus turns this into [[consensus]]: a [[particle]] is final when $\phi^*_i > \tau$. [[neurons]] gossip [[cyberlinks]], GPUs iterate $\phi^*$, and finality emerges from the [[topology]] of [[attention]] -- no voting rounds, no leader election, no block ordering

## network model

leaderless. every [[neuron]] computes $\hat\phi^*$ independently from its local view of the [[cybergraph]]. there is no block proposer, no rotation schedule, no single point of serialization. convergence emerges from gossip, not from coordination

foculus operates in partial synchrony: messages arrive within an unknown but finite bound $\Delta$. during asynchronous periods (partitions), no new [[particles]] finalize -- but no conflicting [[particles]] can finalize either, because local $\hat\phi^*$ cannot reach $\tau$ without sufficient global connectivity. safety holds always. liveness resumes when connectivity restores

no neuron ever needs a complete view of $G$. safety and liveness are established relative to a conflict's own $\varepsilon$-support domain $D$ -- the same canonical, content-defined region [[reward specification]] settlement mining already uses -- not the full planetary graph. a neuron finalizes a [[particle]] once it holds [[vec|VEC P2]] completeness proofs covering enough of $D$'s stake-weighted $\phi^*$ mass, never once it has seen some fraction of the whole [[cybergraph]]. this localization is what makes the protocol's guarantees $N$-independent; see [[foculus security at scale]] for the full derivation

## state

each [[neuron]] maintains:

  - the local [[cybergraph]] $G = (V, E)$ -- [[particles]] as vertices, [[cyberlinks]] as weighted edges
  - the current estimate $\hat\phi^*$ -- converging toward the true stationary distribution
  - the finality set $F$ -- [[particles]] whose $\phi^*_i$ has crossed $\tau$
  - the nullifier set $N$ -- nullifiers committed by finalized [[particles]]

a [[particle]] is in one of three states: pending, final, pruned. transitions are irreversible

## state model

the state is the [[cybergraph]] itself. there is no separate ledger. the finalized subgraph IS the canonical state

each [[token]] output is a [[particle]]. spending a [[token]] creates a new [[particle]] that references the input and presents a nullifier: $n = \text{Poseidon}(\text{NULLIFIER\_DOMAIN}, r.\text{nonce}, \text{secret})$. the nullifier is deterministic from the record -- same record always produces the same nullifier

the nullifier set $N$ is append-only. a [[particle]] that presents a nullifier already in $N$ is invalid. this is the double-spend check: a pure function of the [[particle]] data and the current $N$, independent of arrival order

state transitions happen at finalization:

```
on finalize(P):
  for each nullifier n in P.nullifiers:
    assert n ∉ N           // not already spent
    N ← N ∪ {n}           // commit nullifier
  P.outputs become spendable
  conflicting particles → pruned
```

the critical point: transitions apply when a [[particle]] crosses $\tau$, not when it arrives. every [[neuron]] computes the same $\phi^*$ from the same graph, so they agree on which [[particle]] crosses $\tau$ first. the state sequence is determined by the $\phi^*$ convergence trajectory -- not by a sequencer, proposer, or explicit ordering protocol

## conflict definition and detection

### formal definition

two [[particles]] $P_a, P_b$ conflict if and only if:

$$\text{conflict}(P_a, P_b) \equiv (\exists\, n : n \in P_a.\text{nullifiers} \wedge n \in P_b.\text{nullifiers}) \;\lor\; (P_a.\text{author} = P_b.\text{author} \wedge P_a.\text{epoch} = P_b.\text{epoch} \wedge P_a.\text{signal} = P_b.\text{signal})$$

three conflict types:

| type | condition | example |
|---|---|---|
| double-spend | shared nullifier | two [[particles]] spend the same [[token]] output |
| equivocation | same author, same epoch, same signal type | [[neuron]] signs two contradictory [[cyberlinks]] in one epoch |
| resource collision | shared non-fungible input | two [[particles]] claim the same unique resource |

### detection without ordering

conflict detection is a pure function of [[particle]] content. given any $P_a$ and $P_b$, any [[neuron]] can evaluate $\text{conflict}(P_a, P_b)$ by comparing nullifier sets and author/epoch metadata. no ordering information is needed -- only the data itself

each [[neuron]] maintains a local conflict index: nullifier to set of [[particles]] presenting it. when a new [[particle]] $P$ arrives with nullifier $n$:
- if $n$ has no entry: no conflict, index it
- if another [[particle]] $P'$ already presents $n$: tag $(P, P')$ as conflicting

this detection is monotonic: once detected, a conflict is permanent. a [[neuron]] that has seen both [[particles]] will always detect the conflict, regardless of arrival order. a [[neuron]] that has seen only one treats it as non-conflicting -- the safety proof guarantees the unseen conflicting [[particle]] cannot finalize in the meantime

### exclusive support rule

when a [[neuron]] detects a conflict between $P_a$ and $P_b$, it supports exactly one. the honest strategy: support the first-seen [[particle]]. [[cyberlinks]] go only to the supported member of the conflict group. the unsupported member receives no $\phi^*$ mass from this [[neuron]]

this is the critical constraint: each [[neuron]]'s stake-weighted mass flows to at most ONE member of any conflict group. conflicting [[particles]] compete for the same finite mass pool

## fork choice rule

$\phi^*$ is the fork choice rule. when conflicts exist, the [[particle]] with higher $\phi^*_i$ is the canonical choice. this is the outcome of the entire network's link structure converging through the [[tri-kernel]] -- not a vote

why this works: $\phi^*$ integrates all [[cyberlinks]] from all [[neurons]], weighted by [[token]] stake. manipulating $\phi^*$ requires controlling the topology of the [[cybergraph]] itself -- which costs real [[tokens]]. exclusive support ensures conflicting [[particles]] split a finite mass pool rather than duplicating it

the "no ordering" claim, precisely: there is no block proposer, no sequencer, no explicit transaction ordering. the ordering emerges from the $\phi^*$ convergence trajectory. the [[particle]] that crosses $\tau$ first wins -- and which [[particle]] crosses first is determined by the graph topology, which every [[neuron]] can compute independently

## protocol

1. gossip -- [[neurons]] broadcast new [[particles]] + [[cyberlinks]]
2. conflict check -- each [[neuron]] indexes nullifiers and detects conflicts on receipt
3. exclusive support -- for each conflict group, the [[neuron]] links only to its preferred member
4. support switching -- each round, a [[neuron]] with stake on the locally-losing member of a conflict re-points it to the local leader with probability $q$ per round, rate-limited by [[VEC|VEC P6]]'s VDF to at most one re-point per $T_{\min}$ per source. required, not optional: without this step the protocol has no rule that resolves a near-50/50 honest split, and the conflict stalls indefinitely rather than converging. re-pointing signals are consensus-only -- zero mint, zero BTS exposure, $v_\ell=0$ by protocol rule -- so finality timing cannot be gamed for reward; see [[foculus security at scale]] theorem T1
5. local update -- every ~100 ms, GPU-accelerated sparse-matrix x vector refines $\hat\phi^*$
6. finalize -- [[particle]] $i$ becomes final in a [[neuron]]'s view once two conditions hold, both local reads, neither requiring a message: $\hat\phi^*_i > \tau(t)$ within $i$'s $\varepsilon$-support domain $D$, where $\tau(t) = \mu_D + \kappa'\sigma_D$, $\kappa' \in [1,2]$ the adaptive-threshold multiplier -- distinct from $\kappa$, the tri-kernel's own contraction rate, see liveness proof below; and the neuron holds [[vec|VEC P2]] completeness proofs covering enough of $D$'s stake-weighted $\phi^*$ mass that no uncertified source could be hiding enough weight to flip the ranking. nullifiers committed to $N$
7. prune -- conflicting [[particles]] with $\hat\phi^* \leq \tau$ are discarded
8. reward -- validator $v$ earns proportional to $\Delta\phi^*$ contributed

## safety proof: no double finality

theorem: two conflicting [[particles]] cannot both exceed $\tau$ in any correct [[neuron]]'s view

assumption: honest [[neurons]] control $\geq \frac{1}{2} + \delta$ of staked [[tokens]]

proof sketch, domain-local (the rigorous version -- see [[foculus security at scale]] theorems L1, L2 for the full derivation, including the certified-completeness argument this sketch elides):

1. conflicting [[particles]] $P_a, P_b$ form a conflict group inside $\varepsilon$-support domain $D$. each [[neuron]] supports exactly one member (exclusive support)
2. under exclusive support, the gap $\Delta_D = \phi^*_D(P_a) - \phi^*_D(P_b)$ is a function of how domain-local stake splits between the two members, not of iteration count -- the tri-kernel's contraction property converges $\hat\phi^*_D$ *toward* this gap, it does not enlarge the gap itself. an earlier version of this proof claimed contraction amplifies the initial advantage with each iteration; that claim is wrong, and the correct statement is in step 5 below
3. honest [[neurons]] collectively control $> \frac{1}{2}$ of $D$'s stake-weighted mass. under first-seen propagation, one member -- say $P_a$ -- receives honest majority support, giving $\Delta_D>0$
4. a [[neuron]] finalizes only once its uncertified mass within $D$ is below the threshold [[foculus security at scale|security-at-scale]] theorem L2 derives from $\Delta_D$, $\kappa_D$, and $C$ -- below that threshold, no view satisfying the certification condition can see the ranking inverted, which is what makes two [[neurons]] with different partial views unable to disagree on the winner
5. therefore $P_b$ cannot cross $\tau$ in any certified view while $P_a$ can. QED

double-spend prevention follows directly: a [[token]] transfer is a [[particle]]. two conflicting spends present the same nullifier. only one crosses $\tau$. the winner's nullifier enters $N$. the loser is pruned

### edge case: near-symmetric split

if honest support splits close to 50/50 ($\Delta_D \to 0$), the situation does not resolve by contraction -- nothing in steps 1-4 above moves stake once the split is set. this is a missing rule, not a proof gap, and step 4 of the protocol above (support switching) is the fix: losing-side stake re-points at a VDF-rate-limited rate until $\Delta_D$ grows enough to cross the adaptive threshold. the adaptive $\tau$ stalls both particles during this race -- high variance from the near-tie raises $\tau$ for both, so nothing finalizes prematurely -- and [[foculus security at scale]] theorem T1 gives the expected resolution time under the switching rule, with an exponential tail

as a deterministic fallback for the measure-zero exact-tie case ($\phi^*_a = \phi^*_b$ precisely, before any switching): lower $\text{hash}(\text{particle\_data})$ wins. computable by every [[neuron]] independently

### what honest neurons guarantee vs. what they do not

guaranteed:
- conflicting [[particles]] cannot both finalize (safety)
- the winner has more honest support than the loser
- nullifier set is consistent across all honest [[neurons]]

not guaranteed:
- which specific conflicting [[particle]] wins (depends on network propagation -- the adversary has some influence over this via timing)
- that the "better" particle wins in any semantic sense -- the winner is the one that propagated faster or accumulated switching support, not the one that is "more correct"

characterized, not merely bounded:
- how fast the conflict resolves: for a clear initial split, by the domain-local spectral gap ([[foculus security at scale]] theorem S5); for a near-symmetric split, by the support-switching race ([[foculus security at scale]] theorem T1) -- both are now derived quantities, not open questions

## liveness proof

ergodicity of the transition matrix $P$ guarantees every valid [[particle]] accumulates $\phi^*$ mass over time

the tri-kernel contracts on every topology, unconditionally: worst-case rate $\kappa_{\max} = 1-\lambda_d(1-\alpha) < 1$, where $\lambda_d$ is diffusion's kernel weight and $\alpha$ its teleport probability -- see [[convergence]] for the composite $\kappa(\lambda_2)$ formula. this means the iteration never stalls on any graph; only its speed varies. convergence rate for a specific conflict's domain $D$ depends on $D$'s own local spectral gap $\lambda_2(D)$: expected time to finality is $O(\log(1/\varepsilon)/\lambda_2(D))$ iterations, and because $D$ is content-bounded ([[reward specification]] §7's $\varepsilon$-support), this rate does not degrade as the planetary [[cybergraph]] grows -- see [[foculus security at scale]] theorem S5 for the domain-local liveness bound, which replaces an earlier, unlocalized version whose worst case scaled with planetary $N$

during partitions: $\lambda_2(D)$ drops for a disconnected domain, finality for particles inside it slows or halts. this is the correct behavior -- the system refuses to finalize when it lacks sufficient local information, not when it lacks global information it never actually needed

## sybil resistance

$\phi^*$ is weighted by staked [[tokens]], not by node count. creating 1000 [[neurons]] with zero stake produces zero $\phi^*$ influence. creating fake [[cyberlinks]] without stake backing produces negligible mass shifts

the cost of attacking $\phi^*$ is the cost of acquiring $> \frac{1}{2}$ of staked [[tokens]] -- same economic security model as proof-of-stake, but the attack surface is the graph topology rather than a voting protocol

## graph connectivity

liveness (above) assumes the [[cybergraph]] has a spectral gap bounded away from zero. this is not a free consequence of honest stake majority: [[cyberlinks]] are content assertions, and a [[neuron]] links what it finds semantically related, not what maintains graph-theoretic connectivity. an adversary without any stake can partition the graph into semantically isolated regions simply by ensuring no content spans them

the honest claim is conditional, not unconditional: wherever genuine semantic overlap exists across a sparse region, karma-weighted reward ($\propto \Delta\phi^+$) makes bridging it pay more than duplicating an already-dense cluster, so rational honest majority closes any such gap in equilibrium. where no genuine overlap exists, a sparse region is not an attack -- it is the graph's true epistemic structure, and no bound should be claimed there. see [[foculus security at scale]] theorem T2 for the equilibrium argument and its residuals (market-lag pricing delay, the directed-Cheeger reversibilization factor)

## finality

foculus provides deterministic finality: once $\phi^*_i > \tau$, the [[particle]] is final. no rollbacks, no probabilistic confirmation depth

### adaptive threshold formula

$$\tau(t) = \mu_{\phi^*} + \kappa'\sigma_{\phi^*}$$

where $\kappa' \in [1,2]$ is the adaptive-threshold multiplier -- notationally distinct from $\kappa$, the tri-kernel's contraction rate (liveness proof, above), a collision the original draft of this document did not disambiguate. the threshold adapts to the current distribution. when the network is decisive (low variance), $\tau$ is low and finality is fast. when the network is uncertain (high variance), $\tau$ rises and finality slows -- the system self-regulates

## performance comparison

| metric | classic BFT | nakamoto | foculus |
|---|---|---|---|
| leader | rotating proposer | miner (PoW lottery) | none |
| finality | 5-60 s | ~60 min | 1-3 s (hub domain), tens of s (sparse-fringe domain) -- see below |
| throughput | 1k-10k tx/s | ~10 tx/s | ~10^9 signals/s per GPU |
| validator scale | 10^2-10^3 | unbounded | unbounded |
| fault tolerance | 1/3 stake | 51% hash | 1/2 $\phi^*$ |

each iteration is a sparse matrix-vector multiply -- embarrassingly parallel, no sequential bottleneck. single GPU (A100): ~50M edges at 40 Hz = 2x10^9 edge ops/s. with $K$ shards, throughput scales linearly

latency, clear-split case: compute ~0.2 s, 5-8 iterations, propagation ~0.4 s, finality ~1.4 s WAN for a well-connected (hub) domain -- this is the *typical*, not worst, case. actual worst-case finality is domain-dependent, not a single number: a sparse-fringe $\varepsilon$-support domain's local spectral gap can push this into the tens of seconds ([[foculus security at scale]] theorem S5), and a near-50/50 honest split adds the support-switching race's own resolution time on top (theorem T1, $O(T_{\min}\log(1/\Delta_{\text{noise}})/(q\delta_{\text{stake}}))$) -- both bounded, neither reducible to the single "~1.4s WAN" figure this table used to state unconditionally

## economics

rewards track the measurable shift in $\phi^*$, but not as a raw proportion -- this section previously stated $\text{reward}(v)\propto\Delta\phi^*(v)$, which is the right intuition and the wrong formula. [[reward specification]] defines the actual mechanism: the reward primitive is the *directed* focus impulse $\Delta\phi^+ = [\Delta J]_+$ (downhill movement only, magnitude alone is not paid for, since raising free energy -- adding noise -- must not earn), divided among overlapping contributors by their [[Shapley value]] over the surprise-weighted value $v^\star$, where surprise is [[Bayesian Truth Serum|BTS]]-scored so a copy earns nothing however large its raw $\Delta\phi^+$. validators who add [[cyberlinks]] that meaningfully and *surprisingly* shift the stationary distribution earn more; validators who duplicate consensus earn nothing regardless of stake. this is what "aligns incentives" means precisely -- not participation, not raw movement, but fair-divided, novelty-gated contribution to convergence

damping prevents concentration: $\phi^*_i \leftarrow \phi^*_i \cdot \gamma^t$, $\gamma \in (0,1)$. older or less-endorsed [[particles]] fade. the system forgets noise and retains what matters

see [[reward specification]] for the full mechanism -- the value function, Shapley division, propose/settle split, and settlement mining this summary elides.

## open questions

### solved (this document answers)

- what is a conflicting particle: formally defined via nullifier collision and author/epoch equivocation -- a pure function of particle data
- how conflicts are detected without ordering: monotonic local index on nullifiers, independent of arrival order
- what data becomes canonical: the [[particle]] that crosses $\tau$ first wins. finalization commits nullifiers to $N$. every [[neuron]] computes the same $\phi^*$ from the same graph, so they agree
- adversarial honest-split (resolved by [[foculus security at scale]] theorem T1): the protocol was found to have no rule that resolves a near-50/50 honest split at all -- contraction converges toward a fixed point, it does not move stake between two competing ones. the support-switching rule (protocol step 4) closes this, VDF-rate-limited so the adversary's counter-push is bandwidth-bounded, not stake-bounded
- convergence time under conflict (resolved for the clear-split case by theorem S5, for the near-symmetric case by theorem T1): expected time to finality is now a derived quantity in both regimes, not an open bound

### open (requires further work)

- partition recovery: when two halves of the network reconnect, how quickly does $\phi^*$ reconverge? bounded by the domain-local spectral gap, but practical latency under adversarial partitions is uncharacterized
- threshold gaming: can an attacker oscillate $\sigma_D$ to manipulate the domain-local $\tau_D$? the adaptive threshold needs formal bounds on adversarial variance injection -- not directly addressed by any result in [[foculus security at scale]]
- pre-finality state reads: before a conflict resolves, applications see ambiguity. the [[particle]] with higher current $\phi^*$ is the best guess, but it may change. specifying a safe API for pre-finality state queries (optimistic vs. pessimistic reads) is needed
- cross-particle dependencies: if $P_c$ depends on $P_a$'s output, and $P_a$ conflicts with $P_b$, then $P_c$ cannot finalize until $P_a$ does. long dependency chains affect throughput -- quantifying this is open
- MEV within finality window: if multiple non-conflicting [[particles]] finalize in the same epoch, their relative ordering (by $\phi^*$ value) determines application state. extractable value from link timing needs analysis
- bootstrapping: a cold network has few [[cyberlinks]] and small local spectral gap -- finality may be slow until a region of the [[cybergraph]] reaches sufficient density. related to but not resolved by `roadmap/hub-domain-size-bound.md`, which characterizes domain size at the opposite end (already-dense hubs), not the cold-start case
- eight further items, each independently scoped with a known or partial resolution path: see `roadmap/README.md`

---

[[consensus]] is computed, not voted

see [[collective focus theorem]] for convergence proofs. see [[tri-kernel]] for the operators. see [[focus flow computation]] for the full protocol specification. see [[cyber/state]] for the world state model. see [[cyber/security]] for the nullifier security proof. see [[foculus beacon]] for the epoch randomness beacon derived from the finalized set. see [[foculus security at scale]] for the domain-local safety and liveness derivations this document summarizes normatively -- theorems L1, L2, T1, T2, S4, S5 -- and `roadmap/README.md` for what those theorems leave open
