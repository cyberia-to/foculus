---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
---
# derive springs and heat worst-case sensitivity

the tri-kernel's Lipschitz constant $C\leq2.25$ has one derived term and two asserted ones.

## the gap

[[foculus security at scale|security-at-scale]] theorem L1 computes the diffusion operator's per-node worst-case perturbation directly: removing an edge $(i\to j,w_{ij})$ changes mass by exactly $2\alpha\cdot w_{ij}/W_i\cdot\phi[i]$, verified by direct computation of the renormalization. this gives the $2\alpha$ diffusion contribution to $C$.

the springs and heat operators are asserted to "contribute analogously" — folded into the composite bound $C\leq2\lambda_d\alpha+2\lambda_s+4\lambda_h=2.25$ as an upper bound, summed independently rather than jointly optimized. unlike diffusion, neither operator's own per-node worst-case form has actually been computed. the claim rests on both being local operators with bounded per-edge sensitivity — plausible, since that is true of diffusion too, but plausibility is not the derivation L1 gives diffusion.

## why it matters

$C$ is the single constant every concrete number in [[foculus security at scale|security-at-scale]] is built on — $\Phi_{\text{uncert,max}}^{(D)}=0.00393$, the 99.6% certification target, S4's round count. if the true per-node worst case for springs or heat differs in shape from diffusion's (for instance if heat's two-hop structure produces a worse constant than the doubled coefficient already assumes), every downstream number needs recomputing, the same way the whole document's numbers shifted once $C$ moved from 2 to 2.25 the first time this constant was tightened.

## what remains

compute the per-node worst-case perturbation for the springs operator (screened Laplacian, mean-neighbor form) and the heat operator (two-hop, $A\times(A\times\phi)$) directly, the way diffusion's was computed — an explicit edge-removal calculation, not an appeal to "local operator, bounded sensitivity." confirm whether the three operators' worst cases can occur simultaneously at the same node (the current $C$ sums them as if they can, which may itself be a further overcount worth checking once each term is properly derived).

see [[foculus security at scale]] theorem L1. see [[tri-kernel]] for the three operators.
