# foculus roadmap

open problems, one file each. not phased milestones — specific unsolved questions, each with enough context to pick up independently. every item here originates from [specs/security-at-scale.md](../specs/security-at-scale.md)'s adversarial review, kept as a brief pointer there and expanded here.

| file | closes / clarifies |
|---|---|
| [source-set-anchoring.md](source-set-anchoring.md) | the never-announce withholding vector — $\Phi_{\text{uncert}}^{(D)}$ needs a closed source list, not gossip discovery |
| [domain-composition-mass-weighting.md](domain-composition-mass-weighting.md) | whether S4's $f_\times$ inherits the adversarial-concentration bug L1's $\varepsilon_v^{(D)}$ form had |
| [springs-heat-sensitivity.md](springs-heat-sensitivity.md) | the Lipschitz constant $C\leq2.25$ — diffusion's term is derived, springs/heat's are asserted |
| [honest-split-anti-concentration.md](honest-split-anti-concentration.md) | theorem T1's escape-from-symmetry step — analogy today, needs a derived lemma |
| [hub-domain-size-bound.md](hub-domain-size-bound.md) | no bound exists on how large an $\varepsilon$-support domain gets around a hub |
| [kappa-min-derivation.md](kappa-min-derivation.md) | the contraction floor $\kappa_{\min}\approx0.425$ is an estimate; the ceiling $\kappa_{\max}=0.925$ is rigorous |
| [spectral-gap-residuals.md](spectral-gap-residuals.md) | T2's market-lag and directed-Cheeger-reversibilization gaps |
| [collusion-resistance.md](collusion-resistance.md) | BTS and T2 are both unilateral arguments; coordinated deviation is open at the [[tru]] layer |

items move here once specs/ has landed a result and adversarial review has found what's still open around it — not before. see [specs/protocol.md](../specs/protocol.md) and [specs/security-at-scale.md](../specs/security-at-scale.md) for what's already closed.
