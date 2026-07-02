---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
---
# anti-concentration lemma for the support-switching race

theorem T1 needs a formal escape-from-symmetry argument; it currently has an analogy.

## the gap

[[foculus security at scale|security-at-scale]] theorem T1 closes the honest-split stall (foculus as originally specified has no rule that resolves a near-50/50 honest split — the "contraction amplifies the gap" claim in an earlier draft was simply wrong, since contraction acts on convergence toward a fixed point, not on the gap between two competing fixed points of a static graph) by adding a support-switching rule: losing-side stake re-points toward the local leader at VDF-rate-limited probability $q$ per round.

the proof sketch has three steps. steps (ii) and (iii) — the drift once a gap exists exceeds the adversary's VDF-bounded counter-push, and the threshold-crossing time once the drift is established — are solid, standard martingale mechanics. step (i), the initial escape from the symmetric point $\Delta_0\approx0$, is asserted by structural analogy to Avalanche-family consensus's metastability resolution, not derived for this specific switching process.

## why it matters

step (i) is what makes the theorem's headline claim — expected finality time, with an exponential tail — actually go through. without a derived anti-concentration bound, "the symmetric point is unstable" is a reasonable expectation, not a proof, and the exponential-tail claim in particular needs the escape time distribution to be controlled, not merely typical.

## what remains

derive the anti-concentration lemma for this specific process: independent per-neuron switching decisions under drift $qH\cdot g(\Delta_t)-a_t$, starting from $\Delta_0\approx0$, show $|\Delta_t|$ exceeds the noise floor $\Delta_{\text{noise}}=\Theta(\sqrt{q/S_{\text{atoms}}})$ within $O(1/q)$ rounds with the stated probability. this must be discharged *after* — and using — the economic-neutrality rule already required of the switching signal (zero mint, zero BTS exposure, $v_\ell=0$): a rewarded switch is a different stochastic process than the neutral one this lemma needs, since a strategic neuron's switching decision would then depend on more than the local view of $\Delta_t$.

see [[foculus security at scale]] theorem T1. the Avalanche/Snowball family (Rocket et al.) is the structural analogy to formalize against, not to import wholesale.
