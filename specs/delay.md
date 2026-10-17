---
tags: foculus, protocol, crypto
crystal-type: spec
crystal-domain: cyber
alias: delay, vdf, verifiable delay, verifiable delay function
---
# delay — prove that time passed

proves minimum wall-clock time elapsed between two events. inherently sequential — cannot be parallelized. physical time without clocks, without NTP, without trusted timestamps: a verifiable delay function (VDF).

foculus owns the primitive because every use of it is an ordering question — [[structural sync]] layer 2, the [[foculus beacon|beacon]], the cost of equivocation. it moved here from [[mudra]] on 2026-10-07 ([[soft3/roadmap/component-boundaries|component boundaries]]); mudra keeps confidentiality (seal, stealth, veil, quorum) and nothing about time.

## status

| | |
|---|---|
| implemented | `src/vdf.rs` — `evaluate(input, iterations) → VdfProof`, `verify(proof)`, `challenge_from_hash(h)`: $T$ sequential squarings over Goldilocks, verified by recomputation |
| gap | Goldilocks has known order, so repeated squaring in $\mathbb{F}_p$ is sequential work but **not** a VDF: verification costs as much as evaluation, and $x^{2^T}$ reduces to one exponentiation by $2^T \bmod (p-1)$. the group-of-unknown-order construction below is the specification; the code fixes the interface and the chain binding until it lands |
| $T_{\min}$ | undetermined — see [[foculus/specs/parameters|parameters]] |

## interface

```
vdf_prove(input: H, T: u64) → VDFProof
vdf_verify(input: H, T: u64, proof: VDFProof) → bool
```

- `input`: hemera hash of the previous signal (or any commitment)
- `T`: minimum sequential steps (difficulty parameter)
- `proof`: the output plus auxiliary data for fast verification

## mechanism

repeated squaring in a group whose order is unknown to the prover:

```
y = x^(2^T) mod N
```

computing `y` requires `T` sequential squarings. verifying `y` requires O(log T) operations with a Wesolowski or Pietrzak proof.

the field itself has known order, so repeated squaring in $\mathbb{F}_p$ alone is not a VDF. three candidate constructions:

**option A: class group VDF (transparent)**
- group: class group of an imaginary quadratic field
- no trusted setup (group order unknown by construction)
- slower evaluation, but transparent
- aligned with cyber's no-trusted-setup philosophy

**option B: RSA-based VDF (with setup)**
- group: Z/NZ where N = pq (RSA modulus)
- requires trusted setup or MPC for N generation
- faster evaluation
- trusted setup conflicts with cyber's design

**option C: isogeny-based VDF (via genies)**
- group: commutative group action on supersingular curves (the same $\mathbb{F}_q$ as mudra's stealth)
- no trusted setup (group order unknown by construction)
- shares algebra with stealth — one field for both privacy and time proofs
- an isogeny walk is inherently sequential (endomorphism computation cannot be parallelized)

recommendation: option C (genies) or option A (class group). both transparent, no trusted setup. option C reuses the genies infrastructure stealth already needs.

## properties

| property | value |
|----------|-------|
| sequentiality | T sequential squarings, cannot parallelize |
| verification | O(log T) group operations |
| proof size | O(1) group elements (~256 bytes) |
| security | unknown group order assumption |

## usage in foculus

**signal rate limiting:**
```
signal.vdf_proof = vdf_prove(H(prev_signal ‖ body), T_min)
```
each signal requires `T_min` wall-clock time since its predecessor. flooding N signals costs N × T_min sequential time.

**equivocation.** a VDF output is reusable for the same input and difficulty, so two signals on the same `prev` do *not* cost two evaluations — the earlier text that claimed "total VDF time exceeds elapsed time → detectable" was wrong (mudra audit, 2026-09-16). what makes a fork detectable is the chain rule in `src/chain.rs`: one accepted successor per `prev`, the duplicate is equivocation evidence. what makes a fork *costly* is the branch-bound input above — the VDF input commits to the signal body, not only to `prev`, so a second body needs a second evaluation.

**physical time ordering:**
between causally independent signals, VDF proofs create a partial physical-time ordering without any clock synchronization. the [[foculus beacon|beacon]] is the outer VDF over finalized signals.

## parameters

`T_min` is per-neuron configurable:
- phone: longer `T_min` (slower hardware, fewer signals per unit time)
- workstation: shorter `T_min` (faster hardware)
- the VDF adapts to the hardware — honest nodes set `T_min` proportional to their computational capacity

the protocol value is open in [[foculus/specs/parameters|parameters]].

## dependencies

- genies: $\mathbb{F}_q$ group action (isogeny-based VDF) or class group arithmetic
- hemera: input hashing (H(prev_signal ‖ body))
- nebu: Goldilocks field arithmetic (proof verification folds into the zheng accumulator)
- [[mudra/specs/quorum|quorum]] (mudra): optional DKG for group parameter generation

## open questions

- class group vs RSA group (recommendation: class group for transparency)
- exact `T_min` calibration methodology
- VDF ASIC resistance
- proving VDF verification inside a zheng circuit
- the branch-bound challenge and the explicit selection rule between two VDF-valid successors need their own argument (audit item)

[[foculus]] · [[structural sync]] · [[place]] · [[foculus beacon|beacon]]
