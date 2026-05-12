---
tags: reference, research
crystal-type: article
crystal-domain: cyber
date: 2026-03-23
---

# provable consensus circuit specification

## the claim

[[foculus]] consensus -- the fixed point of the [[tri-kernel]] composite operator over the [[cybergraph]] -- can be proven correct inside a [[zheng]] circuit. a single proof (~50 us to verify) replaces all voting rounds, all message passing, all quorum checks. consensus becomes computation.

the [[tri-kernel]] is three operators:

$$\phi^{(t+1)} = \text{norm}[\lambda_d \cdot \mathcal{D}(\phi^t) + \lambda_s \cdot \mathcal{S}(\phi^t) + \lambda_h \cdot \mathcal{H}_\tau(\phi^t)]$$

- $\mathcal{D}$ (diffusion) -- where does probability flow? [[random walk]] on weighted adjacency
- $\mathcal{S}$ (springs) -- screened Laplacian: what satisfies structural constraints? mean neighbor [[focus]]
- $\mathcal{H}_\tau$ (heat) -- multi-scale smoothing: what does the graph look like at resolution tau? 2-hop context

the composite converges to a unique fixed point $\phi^*$ by the [[collective focus theorem]]. this $\phi^*$ is [[focus]] -- the consensus ranking.

two ingredients make this possible:
1. $\phi^*$ is deterministic given the graph -- same adjacency produces same $\phi^*$, hence verifiable
2. [[algebraic state commitments]] let the circuit read the graph as polynomial evaluations instead of hash paths -- O(|E|) field ops instead of O(|E| x log n) [[Hemera]] hashes

without algebraic NMT, provable consensus is impossible in practice -- hemera cost inside the circuit is prohibitive. with algebraic NMT, it fits within [[zheng]]'s capacity with room to spare.

## tri-kernel operators in circuit

### iteration

$\phi^*$ is the fixed point of the composite [[tri-kernel]] operator. each iteration applies three sparse matrix operations and combines them:

```
D_step:  d[j] += α × w_ij × φ[i] / out_degree[i]    # diffusion (PageRank)
         d[i] += (1-α) / |P|                           # teleport

S_step:  s[i] = Σ_j (A_ij + A_ji) × φ[j] / degree[i] # springs (mean neighbor)

H_step:  h = A × (A × φ)                               # heat (2-hop)
         h[i] /= degree²[i]                            # normalize

combine: φ_new = normalize(λ_d × d + λ_s × s + λ_h × h)
```

default weights: $\lambda_d = 0.5$, $\lambda_s = 0.3$, $\lambda_h = 0.2$.

### cost per iteration

each operator is a sparse matrix-vector multiply (SpMV). heat requires TWO SpMV (A^2 phi = A(A phi)).

| operator | SpMV count | edge ops | particle ops | constraints |
|---|---|---|---|---|
| D (diffusion) | 1 | \|E\| muls + divs | \|P\| adds (teleport) | ~8.3M |
| S (springs) | 1 | \|E\| muls + divs (symmetric) | \|P\| divs | ~8.3M |
| H (heat) | 2 | 2x\|E\| muls + divs | \|P\| divs | ~16.6M |
| combine | 0 | 0 | 3x\|P\| muls + \|P\| adds + normalize | ~14.6M |
| total per iteration | 4 SpMV | | | ~47.8M |

### convergence

from the [[spectral gap]] observation: [[bostrom]] converges in 23 iterations (measured contraction kappa = 0.74, lambda_2 = 0.13). the tri-kernel composite has contraction rate $\kappa = \max(\lambda_d \kappa_D, \lambda_s \kappa_S, \lambda_h \kappa_H)$ -- the slowest operator determines convergence. empirically: 23 iterations suffice for all three operators simultaneously.

total constraints for $\phi^*$ computation: 23 x 47.8M = 1.1B

### zheng capacity

[[zheng]] (SuperSpartan + WHIR) handles up to 2^32 = 4.3 billion constraints. 1.1B / 4.3B = 25.6% of capacity. the full tri-kernel uses a quarter of what zheng can prove.

remaining capacity (74.4%): graph reads (algebraic NMT openings), finalization checks (tau threshold), nullifier verification, state transition application.

## cost analysis

### with NMT (current architecture): infeasible

each edge read = NMT inclusion proof = O(log n) [[Hemera]] hashes in-circuit.

```
per edge:    32 hemera calls × 736 constraints = 23,552 constraints
2.7M edges:  2.7M × 23,552 = 63.6 BILLION constraints

zheng capacity: 4.3B constraints
overflow: 15× over capacity
```

the hash cost of reading the graph through NMT destroys the entire approach.

### with algebraic NMT: feasible

each edge read = PCS evaluation = O(1) field operations.

```
per edge:    ~100 field operations ≈ 100 constraints
2.7M edges:  2.7M × 100 = 270M constraints

plus tri-kernel computation: 1,100M constraints
plus finalization:            50M constraints

total:                       1,420M constraints
zheng capacity:              4,300M constraints
utilization:                 33%
```

feasible with 67% headroom. the algebraic representation transforms "read the graph" from a hash-intensive operation to an algebraic one. this is the enabling factor.

the tri-kernel requires reading edges THREE times per iteration (D reads once, S reads once, H reads twice via A^2 phi). with caching of edge values from the first read, subsequent reads are free (the circuit already has the values in witness). effective graph read cost: 270M for the first read, then reuse.

## full circuit (6 sections)

```
PROVABLE_CONSENSUS_CIRCUIT:

inputs:
  BBG_commitment  — polynomial commitment to graph state (32 bytes)
  φ_claimed       — claimed tri-kernel fixed point (focus distribution)
  finality_set    — particles claimed to be final (φ_i > τ)

witness:
  A_edges         — all edge weights (opened from BBG_commitment)
  out_degrees     — per-particle out-degree
  sym_edges       — symmetric adjacency (A + A^T) for springs
  φ_iterations    — intermediate φ vectors for each iteration

constraints:

  // SECTION 1: graph read (270M constraints)
  for each edge (i, j, w) in A_edges:
    assert PCS_eval(BBG_commitment, (axons_out, i, j)) == w
    // one polynomial evaluation check per edge
    // values cached in witness — reused by D, S, H without re-reading

  // SECTION 2: degree and symmetry consistency (5.8M constraints)
  for each particle i:
    assert sum(w_ij for j in outgoing[i]) == out_degree[i]
    assert sym_degree[i] == in_degree[i] + out_degree[i]

  // SECTION 3: tri-kernel iteration (1,100M constraints)
  for t in 0..22:
    // D: diffusion (PageRank)
    for each edge (i → j, w):
      d[j] += α × w × φ_t[i] / out_degree[i]
    for each particle i: d[i] += (1-α) / |P|

    // S: springs (mean neighbor focus)
    for each edge (i, j, w_sym):
      s[i] += w_sym × φ_t[j] / sym_degree[i]

    // H: heat (2-hop smoothed)
    // first pass: h_temp = A × φ_t
    for each edge (i → j, w):
      h_temp[j] += w × φ_t[i]
    // second pass: h = A × h_temp
    for each edge (i → j, w):
      h[j] += w × h_temp[i]
    for each particle i: h[i] /= degree²[i]

    // combine: φ_{t+1} = normalize(λ_d·d + λ_s·s + λ_h·h)
    for each particle i:
      φ_next[i] = λ_d × d[i] + λ_s × s[i] + λ_h × h[i]
    φ_next /= sum(φ_next)
    assert φ_iterations[t+1] == φ_next

  // SECTION 4: convergence check (2.9M constraints)
  assert ||φ_iterations[22] - φ_iterations[21]||_1 < ε

  // SECTION 5: finality check (|F| × 3 constraints)
  for each particle i in finality_set:
    assert φ_claimed[i] > τ(t)
    assert i has valid nullifiers (not in spent set)

  // SECTION 6: output binding
  assert φ_claimed == φ_iterations[22]

total: ~1,420M constraints (33% of zheng capacity)
```

## proof generation times

prover time: 1.42B constraints at ~1 us per constraint (zheng prover on modern hardware) = 1,420 seconds = ~24 minutes.

this is per-epoch, not per-block. one proof covers the full tri-kernel computation for the entire graph state. blocks within the epoch inherit the proven phi*.

with GPU acceleration (SuperSpartan's SpMV is embarrassingly parallel): ~2-3 minutes. practical for epoch boundaries (e.g. every 100 blocks = 500 seconds).

## verification times

verifier time: O(log N) = O(log 1.42B) = 31 Hemera hashes + field ops = 50 us.

one number: 50 us to verify that the complete tri-kernel (diffusion + springs + heat) over 2.9 million particles converged to the correct phi*. on a phone. without downloading the graph.

## what it replaces vs what remains

### replaces

| component | traditional consensus | provable consensus |
|---|---|---|
| voting | 2/3 quorum over N validators | not needed -- pi* is deterministic |
| leader election | rotation, VRF, lottery | not needed -- no blocks to propose |
| message passing | O(N) per round, multiple rounds | not needed -- proof replaces agreement |
| finality gadget | Casper/Grandpa/etc. | not needed -- pi_i > tau is finality |
| sync committee | rotating subset vouches for chain tip | not needed -- proof is self-verifying |
| light client trust | trust sync committee / SPV | trust math -- verify proof in 50 us |
| fork choice | longest chain / heaviest subtree / LMD-GHOST | pi* -- the unique fixed point |
| slashing | detect and punish after the fact | prevention -- invalid pi* cannot produce valid proof |

### remains

1. gossip: [[signals]] (cyberlinks) must still propagate to all validators. the graph must be complete before pi* is computed. gossip is communication, not coordination -- but it is still network traffic

2. graph completeness: validators must agree on WHICH graph to compute pi* over. this is the data availability problem -- solved by [[DAS]] (layer 4 of [[structural sync]]). provable consensus assumes the graph is complete and available

3. signal validity: each signal must be individually valid (layer 1: [[zheng]] proof per signal). provable consensus proves pi* computation is correct -- it does not prove the underlying signals are valid. signal validity is a separate layer

4. economic security: pi* manipulation requires controlling graph topology, which costs stake. provable consensus does not change the economic security model -- it changes the MECHANISM (proof instead of voting) while preserving the SECURITY (stake-weighted)

## recursive structure

provable consensus composes recursively with zheng's folding:

```
epoch 1: prove φ*₁ from graph state G₁
epoch 2: prove φ*₂ from graph state G₂
         FOLD: prove (proof₁ valid ∧ G₂ = G₁ + new_signals ∧ φ*₂ correct)
epoch N: ONE accumulated proof covers ALL history

verification: 50 μs regardless of how many epochs
```

a light client joining at epoch 1,000,000 verifies ONE proof. that proof attests:
- all 1,000,000 graphs were valid
- all 1,000,000 pi* computations were correct
- all finality decisions followed from the correct pi*
- the current state is the result of correctly applying all transitions

this is mathematical certainty compressed to 50 us.

## dependency chain

```
nebu  (Goldilocks field arithmetic)
  ↓
hemera  (Poseidon2 hash — for signal identity, NOT for state reads)
  ↓
zheng  (SuperSpartan + WHIR — proves φ* computation)
  ↓
nox  (16 reduction patterns — SpMV as execution trace)
  ↓
algebraic NMT  (polynomial state — enables O(1) graph reads in-circuit)
  ↓
provable consensus  (φ* proven correct, finality from proof)
```

remove algebraic NMT and graph reads cost O(log n) hemera each, pushing 15x over zheng capacity, making the approach infeasible.

algebraic NMT is the prerequisite. without it, consensus must be voted. with it, consensus can be proven.

## timeline phases

| phase | what | consensus model |
|---|---|---|
| current ([[bostrom]]) | CometBFT (Tendermint fork) | voted, 2/3 quorum, leader-based |
| phase 1 | [[foculus]] on NMT state | computed locally, gossip-verified |
| phase 2 | foculus on algebraic NMT | computed locally, proof-verified per epoch |
| phase 3 | provable consensus | proven once, verified by anyone in 50 us |
| phase 4 | recursive provable consensus | accumulated proof covers all history |

phase 2 is the critical transition. algebraic NMT makes graph reads algebraic. once reads are algebraic, pi* computation fits in a zheng circuit. once it fits in a circuit, it can be proven. once it can be proven, voting becomes unnecessary.

## the deeper implication

consensus has been a PROTOCOL problem since Lamport (1982). nodes communicate to agree. the communication pattern determines safety and liveness. four decades of research optimize the communication: fewer messages (HotStuff), probabilistic finality (Nakamoto), economic incentives (Casper).

provable consensus reclassifies it as a COMPUTATION problem. pi* is a function of the graph. the function is deterministic. the computation is provable. the proof is verifiable. no communication between validators is needed for agreement -- only for data propagation.

this is a category shift. the question changes from "how do nodes agree?" to "can a node prove it computed correctly?" the answer -- enabled by algebraic state commitments and recursive proofs -- is yes. 624 million constraints. 50 microseconds to verify. consensus without consensus.

see [[foculus]] for the consensus specification. see [[algebraic state commitments]] for the enabling primitive. see [[vec]] for the formal consistency model. see [[structural sync]] for the five-layer framework. see [[collective focus theorem]] for the convergence proof. see [[spectral gap from convergence]] for the empirical observation that bostrom converges in 23 iterations
