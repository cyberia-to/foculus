---
tags: docs, core
crystal-type: process
crystal-domain: cyber
---

# foculus between planets

most consensus works like a committee that votes. someone proposes, everyone raises a hand, someone counts to two-thirds, the result is announced. this is fine in one room. between planets it is a letter mailed across half the solar system: Earth to Mars is 4 to 24 minutes one way, and a single voting round needs the reply too. a committee that meets by interplanetary post decides nothing. every leader-and-quorum protocol is physically dead in deep space.

[[foculus]] holds no vote. it settles the way water finds its level.

## the pond

picture a pond. each [[neuron]] drops a pebble -- a [[signal]], a [[cyberlink]] staked between two [[particles]]. rings spread. and here is the whole trick: given the same pebbles, the final shape of the water surface is the same wherever you stand. you never phone the far shore to agree on the water level. physics does it for you.

every node just whispers to its neighbors what it saw ([[gossip]]) and quietly runs the same arithmetic over the rumors it holds (the [[tri-kernel]], computing the attention distribution [[focus|$\phi^*$]]). the operator is a contraction, so every node applying the same rule to the same signals slides toward one and the same fixed point. agreement is computed independently and comes out identical -- like everyone solving one equation reaching one root. a [[particle]] is final when enough attention mass has gathered on it: $\phi^*_i > \tau$. nobody announces this. each node sees it cross locally.

## four consequences, and the whole of interplanetary foculus is in them

### nobody waits for the round trip

with no vote to collect, no node sits idle for 40 minutes awaiting a reply. you act on what you hold. later information refines the same answer; it never overturns a [[finality|finalized]] one. the latency of the link stops being the latency of agreement -- only propagation costs light-time, the mechanism itself costs nothing. foculus is leaderless by construction: convergence emerges from gossip, not from coordination.

### each planet is its own pond

Martian chatter is mostly about Martian things; it settles among Martian nodes at Martian speed. Earth settles at Earth speed. this is the [[reward specification|$\varepsilon$-support]] domain made physical: no neuron ever needs a complete view of the [[cybergraph]], and finality is established relative to a conflict's own local domain, not the whole planetary graph. Mars does not wait on Earth to live. the interplanetary delay is paid only when both worlds speak about the same particle -- a shared contract, a shared fact. then the two ponds reconcile their levels, and that reconciliation costs light-time, but only for that shared item, in a bounded number of cross-domain rounds (see [[foculus security at scale]] S4), after which it is settled forever.

### a broken link breaks nothing

Mars passes behind the Sun and the link goes dark for roughly two weeks at conjunction. the two ponds simply stop trading rings. neither invents a conflicting truth. foculus runs in partial synchrony: during a partition no new particle finalizes -- and no conflicting particle can finalize either, because local $\hat\phi^*$ cannot reach $\tau$ without sufficient connectivity. safety holds always; liveness resumes when the link returns. the Sun clears, rings resume, the ponds re-level. no fork, no rollback. the system refuses to finalize when it lacks the local information it actually needs, rather than guessing.

### finality is "enough has gathered," not "N minutes have passed"

Nakamoto says wait six blocks, about an hour. that stopwatch is meaningless when the message itself takes 40 minutes. foculus finalizes when attention mass in a particle's neighborhood has converged past the adaptive threshold $\tau$ -- a condition on information, not on a clock. it self-adjusts to however slow the medium is. and so the economy does not freeze in the dark: the epoch [[beacon]] keeps grinding its [[VDF]] even through a quiet or partitioned epoch, $b_E = \text{VDF}_T(b_{E-1})$, always defined and always moving.

## a signal walked Earth to Mars

a Martian links two [[particles]] -- a pebble into the Martian pond. rings spread through Martian nodes; in seconds to minutes the Martian $\phi^*$ converges and the fact is final on Mars. Earth has not yet heard it, and that is correct -- the fact is Martian.

a copy travels to Earth over 4 to 24 minutes, store-and-forward, the delay-tolerant regime deep space already runs. signals commute, so out-of-order and delayed delivery still converge to identical [[state]] -- this is [[vec|verified eventual consistency]]. Earth pours the signal into its pond. absent any Earthly conflict, it is absorbed. where it collides with an Earthly claim -- a contest over one resource -- the cross-domain rounds engage: the two ponds exchange levels, each round about one light-delay, a bounded number of times (S4's boundary-conflict bound), then fixed for good. expensive in hours, yet pure speed-of-light physics, paid only for a genuinely shared dispute, once.

## the scope of the claim

the delay-tolerant, partition-safe convergence above is a property of foculus that holds at any latency: the ~1.4s WAN finality figures in [[foculus protocol|protocol.md]] are Earth-scale, and a cross-domain interplanetary conflict pays its bounded round count at light-delay per round -- hours to a day, not seconds. that cost is fundamental, not a defect: only shared conflicts pay it, and the network stays safe throughout. partition recovery under adversarial conditions remains an open question in the protocol; the interplanetary case sharpens why it matters.

foculus between planets is not a committee voting by interplanetary post. it is a pond finding its level -- one per world, reconciling only where the water is truly shared.

see [[foculus]] for the protocol, [[convergence]] for why the fixed point is unique, [[life of a signal]] for the single-latency walkthrough, and [[location proof]] for the companion question of proving where a node physically is across those same distances.
