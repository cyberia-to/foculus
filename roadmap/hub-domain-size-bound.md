---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
---
# the domain-size / mixing-rate / certification-cost tradeoff curve

$\varepsilon$-support domain size is content-dependent by definition; nothing bounds how large it gets around a hub, and every concrete timing number in [[foculus security at scale|security-at-scale]] assumes it doesn't get too large.

## the gap

[[reward specification]] §7 defines the $\varepsilon$-support region as radius $r=O(\log(1/\varepsilon)/\log(1/\lambda_{\text{local}}))$ hops, explicitly "content-dependent — wide around a hub (slow local mixing), tiny on the sparse fringe." [[foculus security at scale|security-at-scale]]'s theorem S5 uses a representative figure of a few thousand significant particles to compute $t_{\text{prop}}\approx3.2\text{s}$ and to argue $t_{\text{iter}}(|D|)$ is a small constant on modern GPU hardware. that figure is asserted, not bounded — a conflict centered on a genuinely major hub could have $|D|$ orders of magnitude larger.

domain size cuts two ways at once: larger $|D|$ means more sources to certify (helps [[vec|VEC P2]] cost only mildly, since each source is still $O(\log n)$ to certify regardless of domain size) but directly hurts both $t_{\text{prop}}$ (more hops, or at minimum more sources within the same hop radius) and $t_{\text{iter}}(|D|)$ (larger SpMV per round). the hub row in S5's timing table ($1$-$3\text{s}$, matching foculus's empirical finality figure) implicitly assumes a domain size in the same range as the sparse-fringe estimate — the two may not actually coincide, and if hub domains are systematically larger, the hub row may be systematically optimistic.

## what remains

derive an actual bound on $|D|$ as a function of hub degree, $\varepsilon$, and $\lambda_{\text{local}}$ — not just the qualitative "wide around a hub" characterization. produce the tradeoff curve: domain size vs. mixing rate (does a larger, denser domain actually mix faster in wall-clock terms despite having more particles, since dense domains tend toward $\kappa_{\min}$) vs. certification cost (source count, not edge count, is the real driver — bound source count as a function of domain size and typical stake concentration). resolve whether the hub row and the sparse-fringe row in S5's table are actually measuring comparable domain sizes, or whether the table needs a third, explicitly-hub-sized row.

see [[foculus security at scale]] theorem S5. see [[reward specification]] §7 for the $\varepsilon$-support definition this inherits.
