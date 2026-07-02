# foculus: reorg for implementation

status: proposed, not executed. written for review before any file moves.

## why now

the protocol is mathematically sound at this point — five rounds of adversarial proof review landed the safety and liveness arguments, closed the structural contradictions, and left a short, honestly-scoped list of residuals. that's the signal to stop writing proofs and start preparing to build. this plan does two things: brings the repo layout in line with what every sibling repo in the stack already converged on, and closes the gap between "what the proofs actually require" and "what the canonical spec says" — which are not currently the same document.

## what's actually here today

```
foculus/
├── README.md
├── reference/            6 files — protocol spec, circuit spec, VEC, beacon, fold mining, scale proofs
└── docs/                 3 files — overview, convergence theory, signal-lifecycle narrative
```

no `roadmap/`. no `specs/` (the implementable-spec directory is called `reference/`, which is the only repo in the stack that calls it that). no `docs/explanation/` nesting. no `.claude/plans/` until this file.

## the convention every sibling repo already uses

checked directly: `mudra`, `nox`, `zheng`, `bbg`, `rune` — all five, no exceptions —

| directory | role |
|---|---|
| `specs/` | implementable, stable, normative — what a builder implements against |
| `docs/explanation/` (+ `guides/`, `tutorials/` as the project matures) | Diataxis narrative layer |
| `roadmap/` | one file per open problem or planned extension — not phased milestones, *specific unsolved questions with enough context to pick up* |
| `.claude/plans/` | plans reviewed before execution, committed once approved |
| `rs/` (or `src/`) | the actual implementation, once specs are pinned |

foculus's `reference/` maps to everyone else's `specs/` in content and intent — this is a naming gap, not a structural one, and the fix is a rename. `docs/` maps to `docs/explanation/` the same way. `roadmap/` is a genuine gap: it doesn't exist, and there's real content that belongs there right now — the open-problems list at the end of `security-at-scale.md` is nine items, each independently scoped, each exactly the shape of a `nox/roadmap/*.md` or `bbg/roadmap/*.md` entry.

## the more important gap: the canonical spec is stale

`specs/protocol.md` (today's `reference/foculus.md`) is the document a new implementer reads first, and it currently describes:

- a safety proof that assumes every neuron sees the same complete graph — since disproven at scale and replaced by the localized, VEC P2-certified argument in `security-at-scale.md`
- no resolution for a near-50/50 honest split — `security-at-scale.md`'s theorem T1 found the base protocol has no rule that moves stake once this happens, and closes it with a new rule (support switching, VDF-rate-limited, declared economically neutral) that **does not appear in the protocol spec at all**
- an unconditional spectral-gap claim implicit in the fork-choice argument, which `security-at-scale.md` showed is false in general and replaced with a conditional theorem

an implementer building strictly from `reference/foculus.md` today would ship the honest-split stall bug, because the fix lives in a different document they may never open. this is the single highest-priority content fix in this plan, independent of any renaming.

## proposed structure

```
foculus/
├── README.md                              updated: points at specs/, docs/explanation/, roadmap/
├── specs/
│   ├── protocol.md                        was reference/foculus.md — AMENDED, see below
│   ├── parameters.md                      NEW — every protocol constant in one place
│   ├── gossip.md                          NEW — the propagation layer, currently just "Assumption G"
│   ├── wire-format.md                     NEW — thin cross-reference to the canonical particle/signal struct
│   ├── provable-consensus.md              unchanged content, moved
│   ├── vec.md                             unchanged content, moved
│   ├── beacon.md                          unchanged content, moved
│   ├── fold-mining.md                     unchanged content, moved
│   └── security-at-scale.md               unchanged content, moved — role shifts to proof appendix, see below
├── docs/
│   └── explanation/
│       ├── overview.md                    moved
│       ├── convergence.md                 moved
│       └── life-of-a-signal.md            moved
├── roadmap/
│   ├── README.md                          NEW — index, one line per item, links to the specs section it extends
│   ├── source-set-anchoring.md            NEW — stake-snapshot lemma, closes the never-announce withholding vector
│   ├── springs-heat-sensitivity.md        NEW — C=2.25 upper bound → derived, not asserted
│   ├── honest-split-anti-concentration.md NEW — T1's escape-from-symmetry lemma
│   ├── domain-composition-mass-weighting.md NEW — does f_× inherit L1's fixed vulnerability
│   ├── hub-domain-size-bound.md           NEW — the domain-size/mixing-rate/certification-cost tradeoff curve
│   ├── spectral-gap-residuals.md          NEW — T2's market-lag and directed-Cheeger gaps
│   ├── kappa-min-derivation.md            NEW — κ_min estimate → proof, or drop the hub-speed row's specific number
│   └── collusion-resistance.md            NEW — cross-linked with tru, since foculus's argument is unilateral same as BTS's
└── .claude/
    └── plans/
        └── reorg-for-implementation.md    this file
```

not proposed this pass: `rs/`. writing implementation scaffolding before `parameters.md` and `protocol.md`'s amendment exist would just produce code against numbers that are about to change. that's phase 2, once this phase lands.

## the protocol.md amendment, specifically

this is content work, not filing. `specs/protocol.md` needs:

1. the seven-step protocol updated to reference domain-local certification (L1/L2's `Φ_uncert^(D)` gate) as the operative finalization condition, replacing the global view-completeness language
2. a new step, or an amendment to step 5, stating the support-switching rule as a normative requirement — not optional, not a future extension: without it the protocol has an unbounded stall under adversarial timing
3. the switching rule's economic-neutrality requirement (zero mint, zero BTS exposure, `v_ℓ=0`) stated as a protocol invariant, since it's a precondition for T1's proof to apply to the real incentive-bearing network rather than an idealized neutral one
4. the fork-choice section's spectral-gap language softened from unconditional to T2's conditional form, with the "genuine semantic disjointness is not an attack" framing preserved — this is a correctness fix, not just an honesty one; the unconditional version is a false claim
5. `security-at-scale.md` re-scoped in its own header as *the derivation record* for these amendments — proofs, worked numbers, residuals — with `protocol.md` as the normative summary a builder actually implements against

## parameters.md — what it needs to contain

every constant currently living as "for this worked example" scattered across six documents, collected once:

- `α` (teleport probability), `λ_d, λ_s, λ_h` (tri-kernel combination weights, sum to 1), `μ` (springs screening constant), `τ` bandwidth parameter, `κ'` (adaptive threshold multiplier)
- `ε` (precision floor / ε-support cutoff), `d` (reorg-stability depth, shared across finality, the beacon, and settlement escrow)
- `T_min` (per-signal VDF delay), `T_beacon` (outer beacon VDF delay), `Δ` (network jitter bound), `q` (support-switching rate)
- epoch window lengths (propose/beacon/settle/fold/tok), if fixed rather than adaptive

most of these currently appear as "with α=0.85..." illustrative plug-ins for worked examples, not as chosen, justified protocol constants. this document is where illustration becomes decision — for each one, either a derivation, an empirical measurement this repo can point to (the bostrom `κ=0.74, λ₂=0.13` pair is the only real one so far), or an explicit "chosen, here's why this value and not another."

## gossip.md and wire-format.md — closing gaps flagged much earlier

the very first pass over this repo (before any of the proof work) flagged three things as missing for implementation: a gossip protocol spec, epoch/parameter values, and a wire format cross-reference. epoch/parameters is covered above. the other two are still genuinely open:

- **gossip.md**: "Assumption G" in `security-at-scale.md` models propagation as a probability bound: it never specifies fanout, push vs. pull, message batching, or what triggers a broadcast. the user's guidance was that this should be cyber-signal-based with no additional hidden layers — that's a real constraint worth writing down as the spec's opening line, then filling in the mechanics.
- **wire-format.md**: the particle/signal struct is asserted to already be defined elsewhere (cybergraph, possibly with tape involvement) — this document doesn't need to define it, it needs to *point at* the canonical definition precisely enough that an implementer doesn't have to go hunting, and confirm the fields this repo's specs assume (`prev`, `merkle_clock`, `vdf_proof`, `step`, `nonce`, the nullifier fields) actually match what's canonical.

## migration mechanics

low-risk, mechanical, `git mv` preserves history:

1. `git mv reference specs`, `git mv reference/foculus.md specs/protocol.md`
2. `mkdir docs/explanation && git mv docs/*.md docs/explanation/`
3. update `README.md` — the two-column reference/docs structure it currently documents, plus a new roadmap section
4. fix the one external cross-reference: `soft3/roadmap/component-boundaries.md:72` points at `foculus/reference/provable-consensus.md`, needs to become `foculus/specs/provable-consensus.md`
5. new content: `parameters.md`, `gossip.md`, `wire-format.md`, `roadmap/README.md` + 8 roadmap files, and the `protocol.md` amendment

## phasing

- **phase 0 (this pass, if approved)**: the mechanical reorg (steps 1-4 above) plus roadmap file extraction (mostly a restructuring of content that already exists in `security-at-scale.md`'s open-problems section, not new derivation)
- **phase 1**: `protocol.md`'s amendment — folding T1/L1/L2/T2 into the normative spec. this is the highest-value content work in this plan and probably deserves its own review pass once drafted, given the track record on this repo
- **phase 2**: `parameters.md` — pinning constants. blocked on real decisions, not just writing; some of these (τ bandwidth, μ, epoch window lengths) haven't been chosen yet, only illustrated
- **phase 3**: `gossip.md` and `wire-format.md`
- **phase 4, not this plan**: `rs/` scaffold, once phases 1-3 give an implementer something stable to build against

## open question for the user before executing

does the merge-into-`protocol.md` direction in phase 1 sound right, or would you rather `protocol.md` stay historically as-published and carry a prominent superseded-by pointer to `security-at-scale.md` instead? the plan above assumes the former — one normative document, proofs elsewhere — because leaving the amendment split across two documents is exactly the kind of gap that caused rework earlier in this repo's life. but it's a real design choice, not a mechanical one, worth confirming before phase 1 starts.
