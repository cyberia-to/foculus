---
tags: foculus, protocol
crystal-type: spec
crystal-domain: cyber
alias: erasure coding, reed-solomon, DAS, data availability sampling
---
# erasure — availability over Goldilocks

Reed-Solomon erasure coding plus data availability sampling: encode content so that any $k$ of $n$ shards recover it, commit each shard with hemera, and let a light peer convince itself the data is retrievable by sampling. this is [[structural sync]] layer 4 and [[vec]] property P3.

foculus owns it because availability is a reconciliation property — "can every peer get the bytes the root commits to" — not a content-identity property. the codec was first proposed as a hemera module (`hemera/roadmap/erasure-coding.md`, retired 2026-10-07): the field argument there still holds, but hemera's job ends at "what is it and how do I verify it"; "can I get it back" is this repo's. hemera hashes the shards; foculus decides how many shards, which to sample, and when the answer is *available*.

## status

| | |
|---|---|
| implemented | `src/erasure.rs` — `encode(data, k, n) → Vec<Shard>`, `decode(shards, k, n, original_len) → Vec<u8>`: polynomial evaluation / interpolation over Goldilocks (nebu) |
| implemented | `src/das.rs` — `commit(shards, k, len) → DasCommitment` (one hemera root per shard), `sample(shard)`, `verify_sample`, `verify_availability`, `confidence_bits(samples)` |
| spec only | fraud proofs for invalid encodings, the 2D grid, Lens openings per cell |

## interface

as implemented:

```
encode(data: &[u8], k, n) → Vec<Shard>          k data + (n−k) parity shards; any k reconstruct
decode(shards: &[Shard], k, n, len) → Vec<u8>   Lagrange interpolation over any k shards

commit(shards, k, len) → DasCommitment          hemera root per shard; the availability commitment
sample(shard) → Sample                          one shard with its root
verify_sample(sample, commitment) → bool
verify_availability(commitment, samples, min) → bool
confidence_bits(successful) → u32               withholding ≥ 50 % survives k honest samples with probability 2^−k
```

target (not built):

```
rs_verify_encoding(commitment, row, cells, k) → bool
  cells consistent with a polynomial of degree < k; otherwise a fraud proof

rs_encode_2d(data, √k) → Grid2D
  √k × √k data grid, rows and columns each RS-extended ×2 → 2√k × 2√k

rs_sample_verify(grid_commitment, row, col, value, proof: LensOpening) → bool
  one cell against the grid commitment via a Lens opening (zheng)

rs_fraud_proof(row_commitment, cells, k) → Option<FraudProof>
  k+1 cells interpolate to degree ≥ k → the encoding is invalid
```

## why one field

1. **same field.** RS over Goldilocks uses the same $\mathbb{F}_p$ arithmetic as everything else; no conversion.
2. **same NTT.** encoding is a domain evaluation — the NTT nebu provides.
3. **pipeline continuity.** data → RS encode → hash shards (hemera) → commit → sample. the encode step sits before the existing hash step.

## what stays outside foculus

- shard hashing and Merkle roots → [[hemera]]
- Lens openings for grid cells → [[zheng]]
- chunk distribution and fraud-proof broadcast → [[radio]]

## cost model

| operation | field ops | notes |
|-----------|-----------|-------|
| RS encode (k symbols) | O(k log k) | one NTT |
| RS decode (k shares) | O(k log k) | one inverse NTT + interpolation |
| fraud proof verify | O(k) | polynomial evaluation at k+1 points |
| 2D grid encode (k² data) | O(k² log k) | 2k row NTTs + 2k column NTTs |

## open questions

- systematic vs non-systematic encoding (systematic = first k symbols are the data; simpler, leaks structure)
- extension factor (2× standard; 3–4× buys availability with bandwidth)
- interleaved coding (polynomials sharing evaluation points — amortizes overhead)
- whether RS shards are leaves of the content tree or replace it for availability

[[foculus]] · [[structural sync]] · [[vec]] · [[hemera]]
