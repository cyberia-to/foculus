---
tags: reference, cip
crystal-type: process
crystal-domain: cyber
status: draft
---

# gossip

the propagation layer [[vec]] P4 assumes exists ("this relies on gossip protocol + [[erasure coding]] reconstruction") but never specifies. this document specifies it, under one constraint stated before any design work began: gossip must be cyber-signal-based, with no additional hidden layers. what follows is the consequence of taking that constraint seriously rather than treating it as a slogan.

## the message is the signal, not a wrapper around it

a gossip message is a [[signal]] -- exactly the structure already defined: `(link, stake, Δφ⁺ claim, σ, prev, merkle_clock, vdf_proof, step, nonce)`. there is no separate gossip envelope, no message-id distinct from the signal's own identity, no gossip-specific header. this is not a simplification for this document's convenience -- it falls directly out of [[vec]] P5 and P6 already being true:

- P5 makes every signal self-certifying: $\sigma$ proves the state transition is valid, so a receiving neuron never needs a gossip-layer integrity check on top of signal verification. one verification step, not two
- P6 makes every signal self-ordering: `prev` chains it to the source's history, `vdf_proof` rate-limits it, `step` gives a monotonic per-source counter. a gossip layer that added its own sequence numbers or acks would be re-deriving structure the signal already carries

a hidden layer would mean: a message wrapper the application layer doesn't see, gossip-specific state a neuron tracks beyond what P1-P6 already require, or a distinct "gossip protocol version" that could diverge from the signal format. none of these exist here. propagating a signal and gossiping are the same act.

## propagation: push, deduplicated by identity

on receiving a signal a neuron has not seen before (checked by the signal's own content identity -- no separate dedup key needed, since two identical signals are the same object under content addressing):

1. verify $\sigma$ (P5, tens of microseconds) and the `prev`/`step` chain position (P6, $O(1)$)
2. if valid and new: apply it (index nullifiers, update local conflict state per `specs/protocol.md` steps 2-3), then forward it to $f$ peers, $f\geq2$
3. if already seen, or invalid: drop. an invalid signal cannot be constructed (P5), so "invalid" here is a defensive check against transport corruption, not an expected case

this is standard epidemic broadcast, and its coverage guarantee is standard too: with fanout $f\geq2$ and $n$ reachable neurons, full coverage completes in $O(\log n)$ rounds with high probability. this is what makes `specs/protocol.md`'s and `specs/security-at-scale.md`'s $t_{\text{prop}}=O(\Delta\log|D|)$ claim a real bound rather than an assumed one -- $|D|$ (not planetary $N$) is the relevant $n$, because propagation only needs to reach neurons who care about domain $D$, which is the next point.

## topology should mirror the cybergraph, not ignore it

a flat, uniform-random gossip overlay -- every neuron peers with a random sample of the whole network -- is the default assumption in most gossip protocols. it is the wrong default here. it would mean every signal about a sparse-fringe domain touching a handful of neurons still gets broadcast toward the whole $N=10^{15}$-particle network before those handful of neurons receive it, defeating the entire point of domain-local safety and liveness.

a neuron should preferentially peer with neurons active in the same or adjacent $\varepsilon$-support domains -- the same locality that already governs everything else in this repo. this is not a separate concept to design: the [[cybergraph]] a neuron already computes over tells it who else is active near a given domain, since $\phi^*_D$ mass concentrates around the same particles that domain-active neurons are linking. peer selection can bootstrap directly from this, rather than from an independent peer-discovery mechanism layered on top. content locality and network locality become the same locality.

this does not mean a neuron only ever hears about its own domains -- some fraction of peering stays random-sample, both for global liveness (a domain no neuron currently tracks still needs to be discoverable) and to prevent domain-affinity peering from becoming an eclipse vector (see open questions). the split between affinity-peering and random-sample peering is a parameter this document does not fix.

## certification is pull, not push

[[foculus security at scale|security-at-scale]] theorem L1's finalization gate needs [[vec|VEC P2]] completeness proofs per source, not just the raw signals. these are a different traffic pattern from new-signal broadcast and should not be forced through the same push mechanism:

- new signals: push, unsolicited, broadcast to keep every interested neuron's view current
- completeness proofs: pull, on-demand, requested by a neuron actively trying to certify $\Phi_{\text{uncert}}^{(D)}$ for a domain it is about to finalize in. a neuron does not need every source's completeness proof for every domain at all times -- only for domains it is currently evaluating a conflict in

this keeps the constant traffic (new signals) small and unconditional, and the bursty traffic (certification, needed only around active conflicts) demand-driven rather than broadcast to everyone regardless of relevance.

## availability: gossip delivers signals, DAS proves they still exist

[[vec]] P4's liveness bound already names [[erasure coding]] as the reconstruction mechanism for lost chunks, and P3's [[vec|DAS]] sampling is the separate mechanism for verifying data still exists across the network without downloading it. gossip's job ends at delivering signals to interested neurons; it is not responsible for long-term availability guarantees, which are P3's layer. a neuron that gossips a signal and then goes offline does not take that signal's future availability down with it -- DAS's erasure-coded redundancy is what survives that, independent of gossip's push mechanics.

## boundary with radio

this document specifies what gets sent, when, and why -- protocol logic. it does not specify connection establishment, NAT traversal, or wire-level transport, which is [[radio]]'s job as the companion P2P transport layer (per `soft3`'s dependency chain: cybergraph submits signals, radio transports them). gossip.md's push/pull patterns and topology preference are requirements radio's peer connections need to satisfy; they are not a redesign of radio itself.

## what this leaves undetermined

the fanout $f$ (stated here only as $\geq2$, the minimum for epidemic coverage), the affinity-vs-random peering split, and any batching threshold for relaying multiple signals in one transport round are not fixed by this document -- they are tuning parameters with real security consequences (too little random-sample peering plausibly weakens the eclipse resistance domain-affinity peering would otherwise trade away) that belong in `specs/parameters.md` once chosen, not asserted here without justification.

eclipse resistance specifically is not analyzed: an adversary controlling enough of a domain's affinity-peered neighborhood could, in principle, selectively delay signals to that domain's genuine participants. VEC P2's completeness certification is the defense against *finalizing* on an eclipsed view (a neuron cannot certify what it cannot pull), but whether affinity-based topology makes eclipse *cheaper to attempt* in the first place is not evaluated here.

see [[vec]] for P1 (convergence, what a neuron does with a delivered signal), P4 (the liveness property this document's mechanism satisfies), P5-P6 (why signals are self-certifying and self-ordering, the reason no wrapper is needed). see `specs/security-at-scale.md` for the domain-local propagation time bound this document's coverage argument grounds, and the certification-query pattern theorem L1 depends on. see `specs/parameters.md` for $\Delta$, fanout, and the other undetermined constants this document introduces or depends on.
