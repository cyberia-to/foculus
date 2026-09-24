---
tags: foculus, audit, launch
crystal-type: audit
crystal-domain: cyber
---
# `src/vdf.rs`: repeated squaring over a known-order field is not a delay function

date: 2026-09-24 · revision: 177fad4 (origin/master) · property: [[launch#property-registry|#6]] beacon unpredictable and unbiasable

## claim in the code

`src/vdf.rs`'s own doc comment: "For signal-scale (local sync), full
re-evaluation is acceptable. For global scale, Pietrzak/Wesolowski proofs
would be needed." This frames the gap as a missing succinctness layer at
scale — the construction itself (`T` sequential squarings `x → x² → x⁴ →
… → x^(2^T)` over Goldilocks) is treated as sound sequential work, just
without a short proof of it yet.

## the actual gap

Repeated squaring is a verifiable delay function only in a group of
*unknown* order — RSA groups, class groups of imaginary quadratic fields.
There, computing `x^(2^T)` in fewer than `T` sequential squarings requires
knowing the group's order (equivalent to factoring, for RSA), which is
exactly what's hard.

Goldilocks is a *field* with public order: `p = 2^64 − 2^32 + 1`, so the
multiplicative group has order `p − 1 = 2^32 · (2^32 − 1)` — small,
completely known, and smooth (its largest prime power factor is `2^32`).
By Fermat/Euler, `x^(2^T) ≡ x^(2^T mod (p−1)) (mod p)` for any `x ≠ 0`.
`2^T mod (p−1)` is one `u64`-scale modular exponentiation regardless of
how large `T` is, and then a single `Goldilocks::exp` on the reduced
exponent (≤ 64 field multiplications) reproduces `evaluate`'s output —
no sequential loop, at any `T`.

The gap is sharper than "there's a shortcut": for `T ≥ 32`,
`2^T mod (p−1)` is periodic in `T` with period exactly 32 (`2^T ≡ 0
mod 2^32` once `T ≥ 32`, and `ord(2)` divides 32 in the odd factor
`2^32 − 1`, so CRT fixes the residue from `T mod 32` alone). Past
`T = 32`, raising `iterations` buys zero additional unknowns — the
full answer key is 32 entries, precomputable once.

## evidence

`src/vdf.rs`, `fast_forward_matches_full_evaluation_for_any_t`: the
`Goldilocks::exp`-on-reduced-exponent shortcut matches real `evaluate`
output at `T` = 10,000,000, 1,000,032 and 63, for three different inputs.
`fast_forward_exponent_has_period_32_past_t_equals_32`: `2^T mod (p−1)`
repeats with period 32 for `T` in `[32, 96)`.

```
$ RUSTC_BOOTSTRAP=1 cargo test --lib vdf
test vdf::tests::fast_forward_matches_full_evaluation_for_any_t ... ok
test vdf::tests::fast_forward_exponent_has_period_32_past_t_equals_32 ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 134 filtered out
```

## why it matters to property 6

The beacon's unpredictability rests on the VDF forcing a minimum
wall-clock delay between a challenge becoming known and a response being
producible — that's what rate-limits grinding and equivocation. If any
node can answer any `(input, iterations)` challenge in a handful of field
operations instead of `iterations` sequential steps, the delay is fiction
regardless of how large `iterations` is tuned: there is no dial in this
construction that restores it, because the flaw is algebraic (a public,
smooth-order group), not a matter of degree.

## remains

Closing this needs an unknown-order group under the sequential squaring
(RSA modulus of unknown factorization, or a class group), which is a
different primitive from anything else in this repo's Goldilocks-native
stack, or a different delay mechanism entirely. That choice is a design
decision, not a bug fix, and isn't made here. `foculus#7`'s binding
hardening (attacker cannot relabel epoch/cluster on a finished VDF) and
this finding are independent: binding constrains what a valid proof is
*about*; this finding is about how cheap producing a valid proof is in
the first place.
