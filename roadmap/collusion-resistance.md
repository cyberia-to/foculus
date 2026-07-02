---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
---
# collusion: the shared gap under every unilateral incentive argument

two of this repo's soundest results — BTS honesty and T2's spectral-gap theorem — are unilateral. neither closes coordinated deviation, and neither should be read as if it does.

## the shared shape

[[Bayesian Truth Serum|BTS]] makes truthful reporting a Bayes-Nash equilibrium *for the report in isolation*: no single neuron improves its expected score by unilaterally misreporting. [[foculus security at scale|security-at-scale]] theorem T2 makes bridging genuine semantic cuts the equilibrium *for a single link-creation decision*: no single neuron profits from redirecting one link away from the karma-maximizing placement. both proofs are correct as unilateral statements. neither rules out a cartel of distinct, real-stake actors coordinating strategy — [[reward specification]] §15 names this directly: "BTS is incentive-compatible only against unilateral deviation."

partial defenses exist on the reward side (the conservation cap limits how much a ring manufacturing consensus on a saturated particle can extract, since a coordinated ring still splits a bounded value; [[karma]] non-transferability limits how much a ring can pool reputation; identity cost limits Sybil-flavored coordination) but none of them are a proof that collusion is bounded, only that some avenues are taxed.

## the composite honesty equilibrium, a related and more specific gap

a proposed theorem — truthful reporting is a Bayes-Nash equilibrium of the *composite* payoff (mint + subsidy + fee + yield + BTS redistribution together, not the BTS report in isolation) — was checked against [[reward specification]] and found to rest on a load-bearing lemma (own-weight monotonicity: a neuron's Shapley share is nondecreasing in its own surprise score) that is not safely dischargeable as stated. §4 of the reward spec documents the general failure mode this lemma needs to rule out directly: *"a normalized fixed point is not monotone in edge weights"* — the reason $v^\star(N)$ can exceed $\Delta\phi^+(N)$, and precisely why the conservation clip exists. this is not a lemma that merely needs writing up carefully; given the spec's own documented counterexample class, it needs either a restricted domain (a linear-response regime near equilibrium, where the non-monotonicity is plausibly negligible) or a genuine counterexample search before anyone should build on it.

## what remains

collusion resistance is a [[tru]]-layer problem more than a [[foculus]]-layer one — both BTS and T2 are unilateral by the same underlying limitation, so a fix likely closes both at once rather than needing two separate arguments. the composite honesty equilibrium's monotonicity lemma is the more specific, more tractable piece: attempt the restricted-domain version first (small perturbations near an already-honest equilibrium), since the counterexample class in §4 is about large, adversarial edge-weight changes, not necessarily small ones.

see [[reward specification]] §4, §5, §15. see [[foculus security at scale]] theorem T2's status note.
