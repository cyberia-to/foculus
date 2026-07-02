---
tags: reference, cip
crystal-type: spec
crystal-domain: cyber
status: draft
---

# foculus parameters

every constant referenced across `specs/`, collected once. most of what follows is not yet a chosen protocol constant -- it is an illustrative value reused across worked examples because reusing one number let those examples compose. this document exists to make that gap visible rather than let it hide as implicit agreement. each entry is marked chosen, derived, illustrative-only, or undetermined, and only the first category is safe to hard-code

## tri-kernel weights and teleport

| symbol | meaning | status | value |
|---|---|---|---|
| $\lambda_d,\lambda_s,\lambda_h$ | diffusion / springs / heat combination weights, sum to 1 | chosen (stated default, [[provable-consensus]]) | $0.5, 0.3, 0.2$ |
| $\alpha$ | diffusion teleport probability | illustrative-only, reused everywhere without derivation | $0.85$ |
| $\mu$ | springs screening constant | undetermined -- no value given anywhere in this repo | -- |
| $\tau_{\text{heat}}$ | heat kernel bandwidth, in $e^{-\tau\lambda_2}$ | undetermined, and collides in notation with the finality threshold $\tau(t)$ -- see notation collisions below | -- |

$\lambda_d,\lambda_s,\lambda_h$ being "default" implies they are meant to be tunable per-deployment, not physically fixed the way $\alpha$'s teleport probability is. neither claim is derived from anything in this repo; both are choices someone made once and every subsequent document reused.

## derived quantities -- not independently set

these follow from the parameters above plus graph structure. listing them here to be explicit they are not knobs:

| symbol | meaning | formula | status |
|---|---|---|---|
| $\kappa(\lambda_2)$ | tri-kernel composite contraction rate | $\lambda_d\alpha+\lambda_s\frac{\|L\|}{\|L\|+\mu}+\lambda_h e^{-\tau_{\text{heat}}\lambda_2}$ | ceiling $\kappa_{\max}=1-\lambda_d(1-\alpha)=0.925$ is rigorous for every topology; floor is an unresolved estimate ($\approx0.425$), see `roadmap/kappa-min-derivation.md` |
| $C$ | tri-kernel Lipschitz sensitivity constant | $\leq2\lambda_d\alpha+2\lambda_s+4\lambda_h$ | upper bound $2.25$ at the weights above; diffusion's contribution is derived, springs/heat's are asserted, see `roadmap/springs-heat-sensitivity.md` |
| $k_{\min}$ | settlement Hoeffding sample count | $\lceil\ln(2/\delta)/(2\varepsilon^2)\rceil$ | derived from $\varepsilon,\delta$ below, not independently chosen -- $\approx72{,}500$ at the illustrative $\varepsilon=1\%,\delta=10^{-6}$ |

the one real measurement in this repo: bostrom's empirical $\kappa=0.74,\lambda_2=0.13$ pair ([[provable-consensus]]), consistent with the composite formula for reasonable $\tau_{\text{heat}},\|L\|$, but a single historical snapshot, not a validated operating range.

## timing

| symbol | meaning | status | value used in examples |
|---|---|---|---|
| $T_{\min}$ | per-signal VDF delay ([[vec]] P6) | undetermined -- referenced symbolically everywhere, no value chosen | -- |
| $T_{\text{beacon}}$ | outer beacon VDF delay ([[foculus beacon]]) | undetermined; beacon.md states only the design constraint (exceeds the settle decision window, fits inside the epoch) | 10s in `docs/explanation/life-of-a-signal.md`'s conservative walkthrough, explicitly illustrative |
| $\Delta$ | network jitter bound | illustrative-only, reused consistently | $0.4\text{s}$ |
| propose / beacon / settle / fold / tok window lengths | epoch phase durations | explicitly undetermined -- [[foculus beacon]] states epoch length is decided elsewhere; `life-of-a-signal.md` frames its numbers (85s conservative, ~25s tuned) as a worked illustration of the tradeoff, not a proposal | see that document's table |
| $q$ | support-switching rate (protocol step 4, new in this pass) | undetermined -- introduced by [[foculus security at scale]] theorem T1, no value chosen; T1's own liveness bound is stated in terms of $q$, not with $q$ resolved to a number | -- |

## precision and depth

| symbol | meaning | status | value used in examples |
|---|---|---|---|
| $\varepsilon$ | precision floor / $\varepsilon$-support cutoff | illustrative-only despite [[reward specification]]'s appendix calling it "a protocol constant" -- no value is actually fixed | $10^{-6}$ |
| $\delta$ | Hoeffding confidence parameter for settlement sampling | illustrative-only | $10^{-6}$ |
| $\delta_{\text{stake}}$ | assumed honest stake margin above $\frac12$ | this is a security *assumption* the proofs are conditioned on, not a protocol-set value -- how much margin to plan for is a deployment decision | $0.05$ used throughout [[foculus security at scale]]'s worked examples |
| $d$ | reorg-stability / finality depth | undetermined -- referenced symbolically across [[foculus beacon]] (beacon stability), [[fold-mining]] (pulse escrow), and `roadmap/source-set-anchoring.md` (stake-snapshot anchoring), all three explicitly sharing one parameter, never a number | -- |
| $\kappa'$ | adaptive-threshold multiplier, $\tau(t)=\mu_D+\kappa'\sigma_D$ | range fixed by `specs/protocol.md` | $\kappa'\in[1,2]$; $1.5$ used in worked examples |
| $\gamma$ | $\phi^*$ damping decay rate | range only | $\gamma\in(0,1)$, no specific value chosen |

$d$ is the single highest-leverage undetermined parameter in this list: it is load-bearing for beacon unbiasability, reward pulse irreversibility, *and* the proposed fix for the source-set enumeration gap, and a wrong choice degrades all three simultaneously rather than independently.

## gossip

| symbol | meaning | status |
|---|---|---|
| fanout | gossip broadcast fanout | lower bound only, "$\geq2$", stated everywhere it's mentioned; specific value undetermined |
| DAS sample count $k$ | data-availability sampling depth | the one parameter in this repo with concrete recommended values, from [[vec]] directly: $k=20\to99.9999\%$ confidence, $k=30\to99.99999999\%$ |

fanout, message batching, push-vs-pull, and broadcast triggers belong to a gossip specification this repo does not yet have -- see the reorg plan's phase 3.

## notation collisions worth fixing before implementation

three symbols are overloaded across this document set, one already partially disambiguated:

- $\kappa$ (tri-kernel contraction rate) vs $\kappa'$ (adaptive-threshold multiplier): disambiguated in `specs/protocol.md` during this pass; carried through consistently in `specs/security-at-scale.md`
- $\tau$ (finality threshold function $\tau(t)$) vs $\tau$ (heat kernel bandwidth, inside $\kappa(\lambda_2)$'s $e^{-\tau\lambda_2}$ term): not yet disambiguated anywhere in this repo. renamed $\tau_{\text{heat}}$ in this document for clarity; the source formula in [[convergence]] and its citations in `specs/security-at-scale.md` still use bare $\tau$ for both meanings and should be updated to match
- $\varepsilon$ (protocol-wide precision floor) vs $\varepsilon$ (generic target accuracy in composition-round formulas, e.g. `specs/security-at-scale.md` theorem S4's $R=\lceil\log(Cf_\times/((1-\kappa)\varepsilon))/\log(1/\kappa)\rceil$): milder overload, same symbol used for a specific protocol constant in some places and a free target-accuracy variable in others. lower priority than the $\tau$ collision but worth a pass

## what this means for implementation readiness

seven parameters ($\mu$, $\tau_{\text{heat}}$, $T_{\min}$, $T_{\text{beacon}}$, epoch window lengths, $q$, $d$) have no chosen value anywhere in this repo -- only symbolic treatment or illustrative stand-ins reused across worked examples because reuse let the examples compose, not because the values were decided. $\alpha$, $\varepsilon$, $\delta$, $\Delta$, and $\delta_{\text{stake}}$ are illustrative-only in the same sense: consistently reused, never justified. only $\lambda_d,\lambda_s,\lambda_h$ (stated defaults) and DAS's $k$ (stated recommendation) are actually chosen values a builder could hard-code today without first making a decision this repo has deferred.

pinning $d$ first is the highest-leverage move, since three other undetermined items (the beacon, the reward pulse escrow, and source-set anchoring) already assume it is shared. $\mu$ and $\tau_{\text{heat}}$ are next, since they are the two terms standing between $\kappa_{\max}=0.925$ (rigorous) and a real $\kappa_{\min}$ (currently only estimated) -- see `roadmap/kappa-min-derivation.md`.

see [[foculus]] for where each parameter is consumed. see [[foculus security at scale]] for the derivations that produced $C$, $\kappa_{\max}$, and $k_{\min}$'s formula. see `roadmap/README.md` for the open items this document's gaps feed into.
