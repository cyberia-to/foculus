---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
---
# does domain composition's $f_\times$ inherit L1's fixed vulnerability

theorem S4's cross-domain fraction $f_\times$ was never re-derived the way L1's view-error bound was, after L1's own edge-weight form was found unsound.

## the gap

[[foculus security at scale|security-at-scale]] theorem L1 originally bounded finalization error by $\varepsilon_v^{(D)}$, the *fraction of edge weight* missing from a neuron's view. this was shown false in the worst case: an adversary can withhold all of a high-$\phi^*$, low-out-degree particle's outbound edges while $\varepsilon_v^{(D)}$ stays arbitrarily small, since edge-weight fraction and $\phi^*$-mass are unrelated quantities in general. L1 was corrected to bound by $\Phi_{\text{uncert}}^{(D)}$, uncertified *mass*, closing the counterexample.

theorem S4 (domain composition) still bounds its cross-domain composition error by $f_\times$ — the cross-domain *edge-weight* fraction of the global graph — invoking the pre-correction form of L1 to justify the bound. the safety-threshold comparison was relabeled for notational consistency ($\varepsilon_{\max}^{(D)}\to\Phi_{\text{uncert,max}}^{(D)}$) but $f_\times$ itself was not re-derived in mass-weighted terms.

## why this likely matters

the same construction that broke L1 plausibly applies here: an adversary positioning a high-$\phi^*$ particle adjacent to a domain boundary could make its cross-boundary influence disproportionate to the edge-weight fraction $f_\times$ assigns it, the same way a low-out-degree high-mass node broke the edge-weight bound in L1. the likely shape of the fix is the same shape too — a mass-weighted cross-boundary influence measure, bounded via the locality tail already established for $\Delta\phi^+$ ([[reward specification]] §2) rather than via raw boundary edge weight.

## what remains

derive S4's composition-error bound directly, the way L1's diffusion term was derived (not asserted), using a mass-weighted cross-domain quantity analogous to $\Phi_{\text{uncert}}^{(D)}$. confirm or refute the suspected vulnerability with an explicit worst-case construction, the way L1's counterexample was explicit. if confirmed, recompute S4's round-count $R_{\text{safe}}$ under the corrected bound — the current figure (18 rounds at $f_\times=0.1$) may not survive.

see [[foculus security at scale]] theorems L1 and S4.
