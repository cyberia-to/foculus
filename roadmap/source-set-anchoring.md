---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
---
# source-set anchoring for $\Phi_{\text{uncert}}^{(D)}$

closes the never-announce withholding vector by replacing gossip discovery with a queryable, committed source list. the shape of the fix is known; it is not yet written as a proven lemma.

## the gap

[[foculus security at scale|security-at-scale]]'s theorem L1 bounds finalization error by $\Phi_{\text{uncert}}^{(D)}$, the $\phi^*_D$-mass held by particles whose in-domain source chains are not yet [[vec|VEC P2]]-certified complete. the proof claims this is computable "entirely from the neuron's own current view" — true only given a correctly enumerated source set, and nothing in P2 establishes that. P2 certifies "everything source $s$ published," never "these are all the sources." a source the neuron has never heard of contributes real missing edges while being invisible to the sum — not flagged uncertified, simply absent. the true $f_i>0$ at some node; the neuron computes $f_i=0$ and silently undercounts.

this is a different failure mode than a known source withholding edges (which L1's evaluation-point argument already handles: the accounting redirects to the withholding source's own view-mass). enumeration is one level above completeness.

## the fix

only stake-backed cyberlinks matter for $A^{\text{eff}}$ ([[reward specification]] §3), and stake positions are settled state. the set of sources holding in-domain stake as of a $d$-deep stable [[bbg|BBG]] root is enumerable directly from that committed root — a closed list, not an open-ended one assembled by hoping gossip surfaced everyone. "all relevant sources certified" becomes checkable against this list.

cost: stake entering within the last $d$ epochs does not count toward $A^{\text{eff}}$ for finalization purposes until it is snapshot-visible. a one-parameter activation delay, structurally identical to the pulse-escrow depth [[reward specification]] already requires — plausibly the same $d$, which would mean no new parameter at all.

## what remains

state and prove: "$\Phi_{\text{uncert}}^{(D)}$ is computable given source-set anchoring at depth $d$" as an actual lemma, not an unqualified claim. specify the BBG query shape (algebraic NMT evaluation against the stake-commitment polynomial, per [[provable-consensus]]'s enabling primitive) precisely enough to implement against. confirm $d$ can genuinely be shared with the pulse-escrow depth rather than needing its own value — if not, derive why they differ.

the never-emit residual is distinct and not addressed by this fix: a correctly enumerated, P2-certified-complete source that simply never publishes a link contributes zero to $\Phi_{\text{uncert}}^{(D)}$ while the graph silently diverges from its semantic ground truth. no completeness mechanism closes this — it is a content problem, not a completeness one, and is the honest floor once this roadmap item lands.

see [[foculus security at scale]] theorem L1. see [[vec]] P2. see [[reward specification]] §3, §7.
