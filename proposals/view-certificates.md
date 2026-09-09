---
tags: foculus, consensus, proposal
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-09-09
alias: view certificate, view-sufficiency certificate, certified finality, observed vs certified, finality certificate
---

# view certificates

> finality as evidence, not declaration: the quorum certificate without the
> committee. a certified finality carries its view, its attesting stake, and
> its challenge window — so a counterparty can verify *how* a state was
> finalized instead of trusting *that* it was.

finality today is a local observation: a node watches its own $\hat\phi^*$ cross $\tau$ and *declares*. nothing in the declaration carries evidence — not of what the node saw, not of who else saw it, not that the threshold was not walked down by a degraded link. a committee protocol pays round-trips for a quorum certificate; foculus refuses the committee and, with it, the certificate. this spec returns the certificate without returning the committee.

the problem, stated in one sentence: **a finality claim is unverifiable by anyone who was not there.**

four consequences follow, and each is a known hole:

1. **self-filtered finality.** a node under an eclipse holds a self-consistent view and crosses $\tau$ honestly. without evidence attached to the claim, no counterparty can tell this finality from a well-connected one. the attacker's freedom is not to lie about $\phi^*$ — it is to choose what the victim saw.
2. **adaptive threshold under pressure.** a $\tau$ that self-adjusts to the medium weakens exactly when the medium is degraded. a finality criterion that slides under DDoS or eclipse hands the filter control of the bar.
3. **no evidence on violation.** two conflicting finalized states in one domain prove an honest-majority violation — but prove it to no one. nothing is slashable because nothing is attributable.
4. **interplanetary import is blind.** across a conjunction, an arriving state is either trusted or not, with no way to express *how* it was finalized. "mars says so" is not a grade.

a view certificate closes all four with one object. it is not a vote about truth — the [[tri-kernel]] still decides what is true. it is evidence about the *window of observation*: that the finalized view was held, held by sufficient stake, and held long enough to be challenged. it makes the honest-majority assumption observable instead of assumed.

## the certificate

a certified finality for particle $i$ in epoch $E$ is the tuple:

$$\mathrm{cert}_i \;=\; \big(\, i,\; \hat\phi^*_i,\; \mathrm{root}_S,\; \sigma_{\Delta\phi},\; \pi_{\mathrm{att}},\; w_E \,\big)$$

- $i$, $\hat\phi^*_i$ — the claim: particle $i$ holds attention mass $\hat\phi^*_i > \tau_E$
- $\mathrm{root}_S$ — an NMT root over the signals in $i$'s $\varepsilon$-support: the view the claim stands on
- $\sigma_{\Delta\phi}$ — a [[zheng]] proof that on that view, $\phi_i > \tau_E$ (the local impulse machinery of [[reward specification]] §2: the support is $O(\log 1/\varepsilon)$ hops, so the proof is small and the recomputation it certifies is local)
- $\pi_{\mathrm{att}}$ — a folded accumulator showing the attesting stake of component 2
- $w_E$ — the VDF window proof of component 3

## component 1 — view binding

the claim commits to the exact set of signals it stands on. the $\varepsilon$-support of $i$ — every node whose contribution to $\Delta\phi^+$ is $\ge \varepsilon$ — is already a canonical, content-dependent superlevel set ([[reward specification]] §7); the signals that built it are committed in the [[bbg]] `signals.root` MMT and namespace-indexed. $\mathrm{root}_S$ is their NMT root.

this binds finality to a view the way a signature binds a message. without it, "final" floats free of any checkable content. with it, verifying the claim is recomputing one local quantity over one committed set — light clients do this as one zheng verification plus a namespace sync, the join path [[bbg]] already specifies.

direction matters: $\mathrm{root}_S$ commits to *at least* the support. a claim that silently excludes a conflicting signal from its committed view is caught by component 3, not by this root — withholding is a challenge-window offense, addressed there.

## component 2 — stake attestation fold

every node, in ordinary [[gossip]], piggybacks attestations: a signature over $(i, \mathrm{root}_S, E)$ for each certified finality it has observed and agrees it saw. attestations are aggregated with the same HyperNova fold accumulator [[reward specification]] §7 uses for settlement tickets; the fold certifies that the attesting set's stake, opened against [[bbg]] `neurons.root`, exceeds $\theta$ times the domain's total stake.

two properties keep this from being a committee in disguise:

- **attestors attest to sight, not to truth.** an attestation says "my view includes at least $\mathrm{root}_S$", not "this is the correct $\phi^*$". no one is asked to choose between conflicting states; the contraction still chooses.
- **conflicting certificates are evidence.** two certificates for the same domain and epoch with different $\mathrm{root}_S$, both over attesting sets $> \theta$, are a cryptographic proof that $> \theta$ of domain stake signed two views — the honest-majority violation, made attributable and slashable. the certificate does not prevent the attack. it makes the attack leave evidence. that is the entire difference between a pond and a protocol a counterparty can rely on.

and the failure mode under attack is correct by construction: a node under eclipse cannot gather attestations, because attestations are gossip and the eclipse is the gossip. it does not certify false finality — it certifies *no* finality and degrades to observed-only. the absence of a certificate is itself information, and it is cheap to produce, which is exactly why uncertified finality must be worth little (see *economy* below).

## component 3 — VDF challenge window, epoch-fixed threshold

finality splits into two grades:

- **observed** — the local crossing $\hat\phi^*_i > \tau$, available instantly, costing nothing, certified by nothing. for low-value acts.
- **certified** — the tuple above, available only after a challenge window $w_E = \mathrm{VDF}_T(b_E \,\|\, i)$ anchored to the [[foculus beacon|beacon]].

inside the window, anyone holding a signal that flips the marginal publishes it with a zheng proof of the flip: the directed impulse $\Delta\phi^+$ of the withheld signal recomputed against $\mathrm{root}_S$'s view, showing $\phi'_i \le \tau_E$. this is the same local impulse primitive as settlement, run once, verifiable without re-executing the tri-kernel. a successful challenge slashes the certifier's stake and voids the certificate; a failed challenge slashes the challenger's bond, paid to the certifier, so griefing the window is not free.

two things this fixes at once:

- **withholding** (component 1's open direction): excluding a conflicting signal from the committed view is precisely what a challenge proves. the self-filtered finality of hole 1 now has a positive cost and a slashing condition.
- **adaptive $\tau$** (hole 2): $\tau$ is fixed per epoch from $b_E$ at certification time and does not move with the medium. DDoS or eclipse can starve a node of observations — degrading its observed grade — but cannot lower the bar for certified grade. the stopwatch claim is preserved where it was honest (observed finality is information-graded) and dropped where it was an attack surface.

the window is latency-native: $T$ scales with the medium, so the mechanism costs light-time only where light-time is the medium, and nothing extra on a planetary LAN.

## what it buys

**planetary.** light clients and full nodes alike verify one O(1) accumulator instead of trusting a claim. conflicting finalities become slashable events with evidence attached, converting an undetectable safety violation into a bounded economic one.

**interplanetary.** the certificate is a small aggregate and travels store-and-forward with everything else. an arriving state carries its grade with it: a receiving planet accepts certified finality, rejects uncertified, and can distinguish them without trusting the sender. adversarial-finalized-but-uncertified state from behind a conjunction cannot be sold across it. partition stops being a blind spot and becomes a well-defined grade: *observed, uncertified, non-importable.*

## integration

| component | builds on | addition |
|---|---|---|
| $\mathrm{root}_S$, support proofs | [[bbg]] `signals.root`, NMT namespaces, `neurons.root` | root in the cert; namespace sync on join |
| $\sigma_{\Delta\phi}$, challenge proofs | [[reward specification]] §2 impulse; [[zheng]] | one local proof per claim and per challenge |
| $\pi_{\mathrm{att}}$ | HyperNova fold ([[reward specification]] §7) | attestation accumulator, $\theta$ threshold |
| $w_E$, $\tau_E$ | [[foculus beacon]] | window per claim; epoch-fixed threshold |
| attestation transport | [[radio]] gossip | piggyback signature |
| value of uncertified state | [[tok]], [[cyb/parts/ward\|ward]] | acceptance policy: certified required above a value class |

this is the economical half of [[provable-consensus]]: that spec proves the whole $\phi^*$ in a circuit; a view certificate proves only that the *support was sufficient and held*, and lets challenges do the disproving. where the full proof makes the prover pay for every node's convergence, the certificate makes the prover commit to a view and the challenger pay only when the view was lying. both are honest; this one is cheap enough to be mandatory.

## residuals — stated, not hidden

1. **$\theta$ is the honest-majority assumption, now observable.** this spec does not remove stake from consensus; it makes a violation produce evidence and uncertified state unsellable. the security claim is strictly weaker-sounding and strictly stronger in practice than "majority is honest": it is "a violation is attributable and an uncertified sale is preventable."
2. **attestation cost.** folding signatures over every certified finality is real work; the $\theta$-fold is amortized over gossip, but parameterization (how much finality traffic the fold budget supports) is open — see [[parameters]] for where the constants land.
3. **griefing by challenge-spam** is bounded by the challenger bond, but the bond size versus the cost of a zheng challenge proof is an open economic parameter; calibrate with the same analysis settlement uses for ticket economics ([[reward specification]] §7, the unmodeled proof-cost condition, which this spec now partially models).
4. **cross-domain certificates.** a certified finality whose $\varepsilon$-support straddles a domain boundary pays the S4 cross-domain bound for its attestations too; the interplay between certificate latency and the boundary-conflict bound is worked out in [[security-at-scale]], and this spec adds one term to it.

## open parameters

| parameter | role | first guess |
|---|---|---|
| $\theta$ | attesting stake fraction of domain stake | $\ge 2/3$, pinned by S4/T1 analysis |
| $T$ | VDF window duration | medium-scaled; planetary ~1 epoch, interplanetary ~one light-delay |
| challenge bond | griefing bound | $\ge$ cost of one impulse proof, tuned with settlement economics |
| max support width | caps $\sigma_{\Delta\phi}$ size | $O(\log 1/\varepsilon)$ already; cap by protocol precision floor |
