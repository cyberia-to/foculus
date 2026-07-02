---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
---
# derive $\kappa_{\min}$, or stop relying on its specific value

the tri-kernel contraction ceiling is rigorous for every topology; the floor used for the hub-domain speed estimate is not.

## the gap

[[convergence]] gives the composite contraction rate as $\kappa(\lambda_2)=\lambda_d\alpha+\lambda_s\frac{\|L\|}{\|L\|+\mu}+\lambda_h e^{-\tau_{\text{heat}}\lambda_2}$. the ceiling $\kappa_{\max}=\lambda_d\alpha+\lambda_s+\lambda_h=1-\lambda_d(1-\alpha)$ is rigorous for every topology, since $\|L\|/(\|L\|+\mu)\leq1$ and $e^{-\tau_{\text{heat}}\lambda_2}\leq1$ universally — this is the strongest fact in [[foculus security at scale|security-at-scale]], an unconditional convergence guarantee independent of $\lambda_2$.

the floor is not symmetric. as $\lambda_2\to\infty$ (idealized, very well-connected) the heat term vanishes, but $\lambda_s\cdot\|L\|/(\|L\|+\mu)$ has no stated $\lambda_2$-dependence in the formula — it depends on $\|L\|$, the full unscreened Laplacian norm, and the screening constant $\mu$, both graph-specific quantities not simply characterized by $\lambda_2$ alone. $\kappa_{\min}=\lambda_d\alpha\approx0.425$, used for the hub-domain speed row in theorem S5's timing table, is an *estimate* that assumes the springs term also becomes small on well-connected domains. plausible — dense hubs typically have small $\|L\|/(\|L\|+\mu)$ too — but asserted, not derived from the formula as it stands. the bostrom-empirical point ($\kappa=0.74,\lambda_2=0.13$) is consistent with the full formula for reasonable $\tau_{\text{heat}},\|L\|$, which grounds the formula itself, but does not pin down the floor.

## what remains

either derive $\lambda_s\cdot\|L\|/(\|L\|+\mu)$'s behavior as a function of $\lambda_2$ directly — likely requires relating $\|L\|$ (largest eigenvalue) to $\lambda_2$ (second-smallest) for the graph structures actually expected in dense domains, which is not automatic since these sit at opposite ends of the spectrum — or drop the specific $\kappa_{\min}\approx0.425$ figure from the hub-domain speed row and report only the rigorous ceiling plus the empirical bostrom point, leaving the hub case as "at least as fast as bostrom-empirical, upper bound not yet derived" rather than a specific number.

see [[foculus security at scale]] theorem S5's timing table. see [[convergence]] for the composite formula.
