---
tags: reference, research
crystal-type: article
crystal-domain: cyber
date: 2026-03-23
---

# provable consensus

## the claim

[[foculus]] consensus — the fixed point of the [[tri-kernel]] composite operator over the [[cybergraph]] — can be proven correct inside a [[zheng]] circuit. a single proof (~50 µs to verify) replaces all voting rounds, all message passing, all quorum checks. consensus becomes computation.

the circuit itself — the kernel in constraints, the algebraic-NMT graph reads, the 1,420M-constraint budget at 33 % of zheng capacity, prover and verifier times — is tru's, because it is the cost of tru's own computation: [[tru/specs/proving|proving]]. this page records what that proof does to a network.

two ingredients make it possible: $\phi^*$ is deterministic given the graph, hence verifiable; and [[algebraic state commitments]] make reading the graph in-circuit O(|E|) field operations instead of O(|E| · log n) hashes. without algebraic NMT the read cost alone is 15× over zheng capacity; with it the whole thing fits with 67 % headroom ([[tru/specs/proving|proving]] § reading the graph).

## what it replaces vs what remains

### replaces

| component | traditional consensus | provable consensus |
|---|---|---|
| voting | 2/3 quorum over N validators | not needed — φ* is deterministic |
| leader election | rotation, VRF, lottery | not needed — no blocks to propose |
| message passing | O(N) per round, multiple rounds | not needed — proof replaces agreement |
| finality gadget | Casper / Grandpa / etc. | not needed — φ_i > τ is finality |
| sync committee | rotating subset vouches for the tip | not needed — the proof is self-verifying |
| light client trust | trust the sync committee / SPV | trust math — verify the proof in 50 µs |
| fork choice | longest chain / heaviest subtree / LMD-GHOST | φ* — the unique fixed point |
| slashing | detect and punish after the fact | prevention — an invalid φ* cannot produce a valid proof |

### remains

1. **gossip.** [[signals]] must still propagate to every validator; the graph must be complete before φ* is computed. gossip is communication, not coordination — but it is still network traffic ([[foculus/specs/gossip|gossip]]).
2. **graph completeness.** validators must agree on *which* graph to compute φ* over — the data availability problem, solved by DAS ([[erasure]], [[structural sync]] layer 4). provable consensus assumes the graph is complete and available.
3. **signal validity.** each signal must be individually valid (layer 1: one zheng proof per signal). provable consensus proves the φ* computation is correct; it does not prove the underlying signals are valid.
4. **economic security.** manipulating φ* requires controlling graph topology, which costs stake. the mechanism changes (proof instead of voting); the security model (stake-weighted) does not.

## recursive structure

provable consensus composes with zheng's folding:

```
epoch 1: prove φ*₁ from graph state G₁
epoch 2: prove φ*₂ from graph state G₂
         FOLD: prove (proof₁ valid ∧ G₂ = G₁ + new_signals ∧ φ*₂ correct)
epoch N: ONE accumulated proof covers ALL history

verification: 50 µs regardless of how many epochs
```

a light client joining at epoch 1,000,000 verifies one proof. it attests that all 1,000,000 graphs were valid, all 1,000,000 φ* computations were correct, every finality decision followed from the correct φ*, and the current state is the result of correctly applying every transition. the per-height fold that exists today is the [[foculus/specs/fold-mining|fold-mining]] tip, not yet this.

## phases

| phase | what | consensus model |
|---|---|---|
| current ([[bostrom]]) | CometBFT (Tendermint fork) | voted, 2/3 quorum, leader-based |
| 1 | [[foculus]] on NMT state | computed locally, gossip-verified |
| 2 | foculus on algebraic NMT | computed locally, proof-verified per epoch |
| 3 | provable consensus | proven once, verified by anyone in 50 µs |
| 4 | recursive provable consensus | accumulated proof covers all history |

phase 2 is the critical transition. algebraic NMT makes graph reads algebraic; once reads are algebraic, φ* fits in a circuit; once it fits, it can be proven; once it can be proven, voting is unnecessary.

## the deeper implication

consensus has been a *protocol* problem since Lamport (1982): nodes communicate to agree, and the communication pattern determines safety and liveness. four decades of research optimise the communication — fewer messages (HotStuff), probabilistic finality (Nakamoto), economic incentives (Casper).

provable consensus reclassifies it as a *computation* problem. φ* is a function of the graph; the function is deterministic; the computation is provable; the proof is verifiable. no communication between validators is needed for agreement — only for data propagation. the question changes from "how do nodes agree?" to "can a node prove it computed correctly?" — and with algebraic state commitments and recursive proofs the answer is yes. 1,420 million constraints. 50 microseconds to verify. consensus without consensus.

see [[tru/specs/proving|proving]] for the circuit. see [[foculus/specs/protocol|protocol]] for the consensus specification. see [[algebraic state commitments]] for the enabling primitive. see [[vec]] for the formal consistency model. see [[structural sync]] for the five-layer framework. see [[collective focus theorem]] for the convergence proof. see [[spectral gap from convergence]] for the observation that bostrom converges in 23 iterations.
