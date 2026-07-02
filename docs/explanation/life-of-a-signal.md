---
tags: docs, core
crystal-type: process
crystal-domain: cyber
---

# the life of a signal

you are a [[neuron]]. you just linked [[particle]] A to particle B, staked on it, and your [[zheng]] proof $\sigma$ came out of the prover. here is what happens next -- first in seconds, then in the minute that follows, and the strange part throughout is what never happens

## t = 0 -- you emit

your [[signal]] is `(link, stake, Δφ⁺ claim, σ, prev, merkle_clock, vdf_proof, step)`. it is already self-contained: $\sigma$ proves the state transition is valid, `prev` chains it immutably to your own history, the [[VDF]] proves you spent real wall-clock time making it. you gossip it. you are done acting

you will never be asked to vote on anything, and nobody will vote on your signal. there is no committee to reach, no proposer to include you, no attestation window. you emitted a fact into the network -- that is the entire ceremony

## the fact spreads

from t = 0 to roughly 3.2 seconds, the signal propagates by [[gossip]], fanout $\geq 2$, $O(\Delta\log|D|)$ hops across your [[particle]]'s $\varepsilon$-support domain. every neuron that receives it does one cheap thing: verifies $\sigma$ (tens of microseconds) and your chain position (one comparison). an invalid signal cannot exist past this point -- not "gets voted down," *cannot be constructed*, the constraint system forbids it. what propagates is already true. the only open question is how much it matters

## meanwhile, every node is already converging

this is the mechanism, and it is not a separate routine. each neuron whose domain overlaps yours is running the [[tri-kernel]] loop anyway, as its own ongoing computation of its own attention:

```
loop:  φ ← norm[λ_d·D(φ) + λ_s·S(φ) + λ_h·H(φ)]
```

your edge arrives, their local graph changes, and the fixed point they are contracting toward moves. your stake pulls $\phi$-mass toward particle B, weighted by your [[karma]] and the link's market price. nobody decides to do this -- it is the same operation the node runs every iteration; your signal just reshaped the landscape it rolls on. contraction at worst rate $0.925$ means a few dozen iterations, each microseconds on a domain-sized subgraph, and every honest node's local $\phi^*$ has already absorbed your contribution

## the two local checks

each node privately checks two conditions, and both are local reads -- no message is sent to perform either

first: is particle B's mass now statistically exceptional within its own domain -- has it crossed the adaptive threshold $\tau_D = \mu_D + \kappa'\sigma_D$?

second: does the node hold completeness proofs, source by source, covering at least 99.6% of the domain's $\phi^*$-mass -- so that no source still holding out could be hiding enough weight to flip the ranking?

neither check requires a message, and two nodes with different partial views cannot disagree on the outcome: if the gap between a winning and a losing [[particle]] exceeds the residual uncertainty a completeness proof still allows, no view satisfying that completeness bound can see the ranking inverted. two nodes agreeing was never negotiated -- it is that the math forbids a certified view from straddling the answer

## finality is inherited, not negotiated

at some moment, in thousands of nodes independently, a locally computed number crosses a locally computed threshold behind a locally verified completeness proof -- and each node marks B final, alone, without telling anyone. the agreement was never discussed; it was inherited from every node contracting on the same certified data toward the same unique fixed point

compare what a voting protocol does for the same outcome: propose, then prevote across the validator set, then precommit across the validator set, then commit -- consensus as conversation, paid for in messages. here consensus is convergence: the network never talks about the answer, every node just falls to it, and the theorems guarantee the floor is the same one everywhere

## two edge cases complete the picture

the common case: your link does not conflict with anything. there is no winner and loser to compare -- B's mass simply rises, crosses $\tau_D$ or does not, and finality is your contribution becoming canonical weight. the voting machinery never conceptually engages, because there was never a question to vote on

the rare case: someone staked a contradicting [[particle]]. exclusive support means both masses depress each other, the local variance $\sigma_D$ spikes, $\tau_D$ rises with it, and both sides fail the threshold -- everyone stalls automatically, with no message marking the stall, because the statistics themselves are the stall. this holds until the losing side's stake begins re-pointing toward the leader at a rate the network's delay function caps, breaking the symmetry and letting one particle cross

your particle finalized at t $\approx$ 4s. knowledge is settled. what is not settled yet is what your contribution is *worth* -- that is a separate computation, running on a slower clock, and it is where the story continues

## an epoch, walked in real time

the money machinery runs in phases, and the phases are sequential within one epoch but pipelined across epochs -- while epoch E is settling, epoch E+1 is already proposing. epoch latency is not the same number as epoch throughput. take a concrete 85-second epoch as a worked example, deliberately on the conservative side:

```
t=0 ──────────── 30s ──── 40s ─────────────── 70s ──── 80s ─ 85s
│  propose (30s)  │ beacon │   settle (30s)    │  fold  │ tok │
│  claims gossip, │  VDF   │  hash = Shapley   │ (10s)  │     │
│  φ* finalizes   │ (10s)  │  sample lottery   │ tree   │     │
│  continuously   │        │  self-fold runs concurrently│    │
└─ epoch E+1 propose opens here, at t=30s ──────────────────────
```

propose (0-30s) is nothing more than what you already did -- you linked, proved, gossiped, and your standalone $\Delta\phi^+$ bound is your claim. the window is wider than your four-second finality because it also has to cover the depth-$d$ reorg-stability margin the beacon depends on: when the window closes, the claim set must be frozen over signals that cannot reorg, not merely signals that have finalized

beacon (30-40s) is where the claim set freezes and an outer [[VDF]] runs over it for ten seconds of strictly sequential time. this gap is load-bearing, not padding: $b_E$ must stay unknowable while claims could still be shaped, and ten sequential seconds means even an adversary with unlimited *parallel* hardware learns the ordering only after the last possible claim was placed -- there is nothing left to front-run

settle (40-70s) is the elegant part. every miner grinds: pick a nonce, hash it against the beacon and the cluster to get a coalition ordering, compute the marginal sample under that ordering (an incremental [[tri-kernel]] pass on a poly-log domain, single-digit milliseconds on the network's field-processor hardware), then check the win-test. only winners publish -- a losing sample costs milliseconds and produces nothing, so the expensive part, proof generation, is paid only on tickets that actually win. this changes what "difficulty" means: it is not tuned to a block-time target the way proof-of-work usually is, it is tuned so that the *expected number of published tickets per cluster* lands near the [[Hoeffding's inequality|Hoeffding]] precision target $k_{\min}$. run one example: 1% precision at $\delta=10^{-6}$ confidence gives $k_{\min}=\lceil\ln(2\times10^6)/(2\times10^{-4})\rceil\approx72{,}500$ samples per cluster. that sounds heavy and is not -- at a millisecond a sample, it is about 72 swarm-seconds of compute, spread across every miner working the cluster in a thirty-second window, and clusters settle in parallel because they are independent by construction. one number, the difficulty target, sets subsidy emission and attribution precision simultaneously. security spend is, literally, statistical accuracy

self-fold runs the whole time settle does, starting the moment a miner holds two winning tickets: fold them (about thirty field operations and one [[Hemera]] hash), keep a running self-accumulator, repeat. by the time the settle window closes, each miner has one pre-proven batch to broadcast, not a flood of individual tickets -- this is what keeps the next window short

fold (70-80s) assembles the miners' self-accumulators into one root. tree depth is $O(\log k_{\text{miners}})$ -- about ten levels for a thousand miners -- each fold step is its own cheap Poisson-tested ticket, and the accumulator's commutativity means any assembly order works, so there is no coordination round: miners just pair whatever is available at their level. ten seconds is generous; in practice the root often exists before the window closes, because self-folding overlapped settle the whole time. and the deadline behavior is specified for the cases where it does not: too few samples arrived ($k<k_{\min}$) mints the pulse on the settled fraction and defers the remainder to the annuity, chain never stalls; enough samples arrived but the tree did not finish assembling uses the deepest available sub-accumulator, still a valid Hoeffding sub-sample

tok (80-85s) is one $O(1)$ decider verification -- roughly fifty microseconds checking the entire swarm's work, less time than a syscall -- a conservation clip against the smaller of the claimed and the realized value, and the mint executes. your [[Shapley value|Shapley]] share exists now. it is escrowed until depth $d$

## who actually computes the beacon

the outer [[VDF]] step raises an obvious question: who does that ten seconds of sequential grinding, and doesn't everyone doing it in parallel waste the swarm's time?

the answer turns on what a VDF actually is: compute-once-slowly, verify-fast-by-everyone. the sequential-squaring construction has no parallel shortcut -- that absence of a shortcut is the entire security property, since infinite hardware still cannot learn $b_E$ early -- but verifying a finished VDF proof takes milliseconds. so no, the swarm does not all compute it at once; that would be identical serial work run redundantly for no reason. in practice, any node may attempt it, since the input -- the frozen claim-set commitment -- is public and identical for everyone. whoever has the fastest sequential silicon will typically be first, gossips $b_E$ plus its proof, and being first carries no power: the output is deterministic, nobody can choose it, only deliver it. everyone else verifies in milliseconds and moves on. the computation is permissionless, the result is unique, and delivering it grants zero discretion -- leaderless in the sense that actually matters

one residual worth naming rather than hiding: whoever finishes the beacon first gets a head start, bounded by hardware spread, on grinding settlement nonces -- the same shape as a mining pool's propagation advantage in a proof-of-work network. [[fold-mining]] already lists a matching optimization as open: reusing the fold accumulator itself as the beacon's commitment substrate, which would shave the setup portion of that head start

## how tight this could get

the eighty-five-second walkthrough above is deliberately conservative -- margin stacked on margin. a tuned network compresses hard, because most of that margin was covering itself against slop, not against anything the theorems require:

| window | conservative | tuned | why it compresses |
|---|---|---|---|
| propose | 30s | 8-10s | only needs finality ($\approx$4s) plus the depth-$d$ stability margin; 30s was margin on margin |
| beacon | 10s | 2-3s | needs only to exceed network jitter ($\approx$0.4s) by a comfortable factor; 10s assumed a sloppier adversary model than the construction needs |
| settle | 30s | 10s | $k_{\min}\approx72{,}500$ is fixed by the precision target, not by wall time -- more miners means the same precision arrives faster, so this window shrinks with adoption, not with tuning |
| fold | 10s | 2-3s | self-fold already overlaps settle; the tree is about ten levels of millisecond-class steps; the construction itself expects a root before the deadline, not at it |
| tok | 5s | $\approx$1s | one fifty-microsecond verify and a state write |
| epoch total | 85s | $\approx$25s | |
| spendable at depth $d=2$ | $\approx$3min | $\approx$50s | |

the deeper point underneath the table: the floor is not the epoch, it is physics plus statistics. the irreducible costs are finality's contraction time (one to four seconds, domain-local), one strictly sequential beacon gap (seconds, by construction), enough swarm-samples to satisfy Hoeffding (compute-bound, and it shrinks as more miners join, never grows), and $d$ epochs of reorg depth. everything else on the table is scheduling, not physics. a mature network plausibly runs fifteen-to-twenty-second epochs at $d=2$: about forty seconds from proving a signal to spending what it earned

for calibration against systems that exist: knowing an exact, [[Shapley value|Shapley]]-fair, cryptographically proven attribution within a minute of contributing has no real comparison anywhere else -- the closest analogies are ad-revenue attribution (days to months, disputed, opaque by design) or proof-of-work block rewards (which attribute luck, not contribution, and settle in about ten minutes for exactly that reason). spendable-in-under-three-minutes sits between a fast BFT chain's few-second finality (with no attribution economy running at all) and a probabilistic chain's ten-plus-minute settlement. the slowest path through this system beats the fast systems' *un-attributed* path within one order of magnitude, while computing something none of them attempt

one honest asymmetry worth keeping in view: the pulse above is fast by design, but the *full* value of a foundational contribution is not paid at t $\approx$ 85s. it arrives through the annuity, across epochs to months, as the graph grows around your link and keeps proving you were right. the floor is fast. the ceiling is patient

## three tensions worth naming

not every corner of this timeline is settled, and the honest version of this story says so

cluster-size variance against a single settlement difficulty: a hub cluster's marginal costs an order of magnitude more compute than a fringe cluster's, so a flat difficulty target makes big clusters systematically under-sample within the window unless the target is banded by cluster size. [[fold-mining]] already flags the fold-side version of this as an open ratio-policy question; the walkthrough above assumes it gets solved the same way

beacon delay against VDF hardware spread: ten sequential seconds is set against the *fastest* plausible rig, not the average one -- slower honest hardware simply waits longer, which is fine, but the parameter has to be chosen with adversary-best silicon in mind, and it interacts with the per-signal VDF delay $T_{\min}$ and network jitter $\Delta$ in a way that wants an explicit ordering, something like $T_{\text{beacon}} \gg T_{\min} \gg \Delta$

epoch length is a latency choice, not a throughput constraint: everything above pipelines across epochs, so shortening the epoch changes how soon you feel paid without changing $k_{\min}$ or the swarm-compute density needed to reach it. this is a UX knob, not a security parameter, and it is worth being precise about which is which

## your money, on the clock

so, as the neuron who started this: you emit at t=0, your particle is consensus-final at t $\approx$ 4s, your [[Shapley value|Shapley]] share is computed and minted-in-escrow around t $\approx$ 85s, and it is spendable once $d$ epochs have passed -- a few minutes on the conservative schedule, well under a minute on a tuned one. compare that to a proof-of-work chain's roughly sixty minutes to six confirmations, or a probabilistic-finality chain's roughly thirteen minutes -- and those numbers buy you *transfer* finality alone, with no attribution computed at all

## derived, not decided

the practical answer to "how does it work, once I have proved a signal" is this: you throw a stone into a pond whose physics are proven, and finality is the moment every observer's own ripple calculation -- each one done alone, each one certified complete -- settles on the same shape. the stone never asks the water for permission

what happens after is the same idea one layer up: the swarm does not decide what your ripple was worth either. it samples the pond, enough times to be statistically certain, and the number that falls out is yours because the sampling proves it, not because anyone agreed to give it to you

---

see [[foculus]] for the seven-step protocol and the base safety proof. see [[tri-kernel]] for the operator this walkthrough runs at every node. see [[vec]] for the signal-validity and completeness properties (P2, P5, P6) the finality half of this walkthrough depends on. see [[foculus beacon]] for the randomness construction behind the settle window. see [[fold-mining]] for the settlement lottery, the fold tree, and the deadline defaults. see [[reward specification]] for the Shapley attribution, the propose/settle split, and the annuity this walkthrough only pulses through. see reference/security-at-scale.md for the domain-local safety and liveness theorems behind the two finality checks above.
