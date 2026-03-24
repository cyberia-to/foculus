---
tags: docs, core
crystal-type: process
crystal-domain: cyber
---

# foculus: consensus by convergence

foculus is the [[consensus]] mechanism where agreement emerges from the mathematics of [[convergence]] rather than from voting, leaders, or message rounds. the [[tri-kernel]] iterates over the [[cybergraph]], redistributing [[focus]] until a unique stationary distribution $\pi$ stabilizes. that distribution IS the consensus.

## the moment a signal becomes knowledge

before consensus, a [[cyberlink]] is a proposal -- a [[neuron]]'s claim about the relationship between two [[particles]]. after consensus, it has [[finality]]. the [[cybergraph]] absorbs it as permanent structure. the signal becomes [[knowledge]]

every [[vimputer]] node applies the same [[signals]] in the same order, converging on identical [[state]]. safety: no two nodes disagree. liveness: the system keeps producing [[steps]]

## why agreement emerges

consensus is an [[equilibrium]], not an axiom. no rule forces [[neurons]] to agree -- [[incentives]] make disagreement costly and agreement profitable

every [[cyberlink]] costs [[focus]] -- a [[costly signal]]. lying wastes finite resources on claims the graph will eventually down-rank. [[bayesian truth serum]] extracts honest beliefs by rewarding predictions that match the crowd's private information. [[karma]] accumulates for those whose signals increase [[syntropy]], decays for those whose signals add noise

the result: rational agents converge to agreement because cooperation dominates defection in the iterated [[game]]. [[consistency]] across the [[cybergraph]] is a [[nash equilibrium]], not a design choice

## finality: the point of no return

once a [[signal]] achieves finality, its [[cyberlinks]] are permanent in the [[cybergraph]] -- the [[focus]] is spent, the link is irreversible

in foculus, finality is deterministic. a [[particle]] is final when $\pi_i > \tau$, where $\tau$ is an adaptive threshold that self-adjusts to network conditions. once crossed, there are no rollbacks, no probabilistic confirmation depths, no waiting for additional blocks

finality is the moment the [[cybergraph]] commits: this signal is knowledge, this link is structure, this state is canonical

## how foculus differs from BFT and Nakamoto

classical BFT consensus (Tendermint, HotStuff, PBFT) works by coordinated voting: a leader proposes, validators vote, and 2/3 agreement produces a block. the cost is coordination -- O(N) or O(N^2) messages per round, leaders, quorums, view changes, finality gadgets. validator sets are bounded to hundreds or low thousands

Nakamoto consensus (Bitcoin, Ethereum PoW) works by competitive mining: miners race to find a valid hash, the longest chain wins. finality is probabilistic -- deeper burial means lower reversal probability, but certainty is asymptotic. throughput is constrained by the orphan rate tradeoff

foculus works by convergence: [[neurons]] gossip [[cyberlinks]], each node independently computes $\hat\pi$ via GPU-accelerated sparse matrix-vector multiplication, and [[particles]] finalize when their $\pi$ mass crosses the adaptive threshold $\tau$. there is no leader to elect, no quorum to assemble, no chain to extend

| property | BFT | Nakamoto | foculus |
|---|---|---|---|
| agreement mechanism | voting rounds | longest chain | convergence of $\pi$ |
| leader | rotating proposer | miner lottery | none |
| finality | deterministic, 5-60 s | probabilistic, ~60 min | deterministic, 1-3 s |
| validator scale | 10^2-10^3 | unbounded | unbounded |
| fault tolerance | 1/3 stake | 51% hash | 1/2 $\pi$ mass |

## the key insight: pi is the fork choice rule

in every consensus protocol, there is a fork choice rule -- the algorithm that decides which of two conflicting histories is canonical. in Bitcoin, it is the longest chain. in Ethereum, it is LMD-GHOST. in Tendermint, it is the latest signed block

in foculus, $\pi$ is the fork choice rule. when two [[particles]] conflict (shared nullifier, equivocation, resource collision), the one with higher $\pi_i$ wins. this is the outcome of the entire network's [[cyberlink]] structure converging through the [[tri-kernel]] -- the sum of all stake-weighted attention, computed independently by every node, yielding the same result everywhere

$\pi$ integrates all information from all [[neurons]], weighted by staked [[tokens]]. manipulating it requires controlling the topology of the [[cybergraph]] itself, which costs real economic resources. exclusive support ensures conflicting [[particles]] split a finite mass pool rather than duplicating it. the [[tri-kernel]] contraction property amplifies initial advantages exponentially -- a slight honest majority compounds into decisive separation

there is no voting. there is no leader election. there is no block ordering. there is a graph, an operator, and a fixed point. the fixed point is consensus.

see [[foculus]] for the complete protocol specification. see [[convergence]] for the mathematical foundations. see [[collective focus theorem]] for convergence proofs. see [[tri-kernel]] for the operators
