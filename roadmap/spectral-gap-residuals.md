---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
---
# T2's two residuals: market lag and directed-Cheeger reversibilization

the spectral-gap conditional theorem holds at equilibrium, under reversibilization; both qualifiers need their own bound.

## market lag

[[foculus security at scale|security-at-scale]] theorem T2 shows that at equilibrium, every sub-threshold cut in the stake-weighted graph is a semantic cut — no persistent structural gap survives rational, karma-maximizing linking, because bridging a genuine cut pays more than duplicating a dense cluster, and the price gate excludes manufactured bridges.

this holds *at equilibrium*. during the price gate's catch-up window — a genuinely novel link has low ICBS price before the market catches on, exactly when its bridging value is highest — the bound holds with $\Phi_{\text{sem}}$ (the true semantic graph's conductance) replaced by the conductance of the *already-priced-in* subgraph, a strictly weaker guarantee. this is [[reward specification]] §12's discovery leak, showing up a second time: the same market-lag mechanism that under-pays an early contrarian-correct link in the reward layer also under-connects the security layer's spectral gap during the same window. one underlying phenomenon, two symptoms in two different documents.

## directed-Cheeger reversibilization

T2's bound is stated on the multiplicative reversibilization of the tri-kernel's diffusion chain, because the actual chain is non-reversible (directed). standard Cheeger-inequality machinery — the tool T2's proof sketch leans on to relate conductance to spectral gap — is proven for reversible chains. the reversibilization step can change the spectral gap by a factor not bounded anywhere in this repo.

## what remains

for market lag: either bound the catch-up window's duration as a function of ICBS's pricing dynamics (shared work with whatever closes the discovery leak in [[reward specification]] itself — this should not be solved twice) or characterize the security degradation during the window explicitly, the way partition recovery is characterized elsewhere in [[foculus]]'s open questions.

for the reversibilization factor: measure it empirically, the same way [[provable-consensus]] reports an empirical $\kappa=0.74,\lambda_2=0.13$ pair for bostrom — a directed-vs-reversibilized $\lambda_2$ comparison on real graph data would tell whether this factor is a rounding error or a real gap in the bound.

see [[foculus security at scale]] theorem T2. see [[reward specification]] §12 for the discovery leak this shares a root cause with.
