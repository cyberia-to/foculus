---
tags: foculus, protocol, crypto
crystal-type: spec
crystal-domain: cyber
alias: place, location proof, proof of position
---
# place — prove where you are

proves geographic position through round-trip time measurements and geometric consistency. four axioms, zero trusted institutions.

position is network geometry — who is how far from whom on the wire — which is a structural property of the sync mesh, so foculus owns it. it moved here from [[mudra]] on 2026-10-07 ([[soft3/roadmap/component-boundaries|component boundaries]]). **spec only — no code.** the claim-precedes-landing doctrine that consumes it is [[cyberia/foundation/space doctrine|space doctrine]].

## axioms

1. **existence**: at least one observer asserts a position (bootstrap anchor)
2. **bounded signal speed**: signals travel at most at the speed of light in the declared medium
3. **spherical Earth**: the embedding is constrained to Earth's surface (known radius)
4. **one honest observer**: at least one node in the RTT mesh reports truthfully

## mechanism

### RTT measurement

```
interface:
  rtt_measure(peer: NeuronId) → Duration
  rtt_to_distance(rtt: Duration, medium: Medium) → Distance

medium types:
  Fiber:    c_eff ≈ 2/3 × c  (refractive index of glass)
  Radio:    c_eff ≈ c         (free space)
  Copper:   c_eff ≈ 2/3 × c  (signal propagation in wire)

distance upper bound:
  dist(A, B) ≤ RTT(A, B) × c_medium / 2
```

RTT bounds distance from above only. a node cannot appear *closer* than it is (signals do not exceed light speed), but it can trivially appear *farther* by delaying its reply. the earlier text here claimed the reverse; the mudra audit (2026-09-16) corrected it. consequence: the mesh proves an upper bound on where you are, never a lower one, and the adversarial model must treat "pretending distance" as free.

### distance matrix

```
interface:
  build_distance_matrix(measurements: [(NeuronId, NeuronId, Duration)]) → Matrix<Distance>
```

N neurons produce N×(N-1)/2 pairwise RTT measurements. each measurement is committed simultaneously via a hemera Merkle tree (prevents selective presentation of favorable measurements).

### classical MDS (multidimensional scaling)

```
interface:
  mds_embed(distances: Matrix<Distance>, dimensions: u8) → Vec<(Fixed, Fixed, Fixed)>
```

given a pairwise distance matrix, classical MDS recovers a 3D coordinate embedding:

1. double-center the squared distance matrix
2. eigendecompose → top 3 eigenvalues/eigenvectors
3. coordinates = eigenvectors × sqrt(eigenvalues)

coordinates are fixed-point over Goldilocks per [[tru/specs/arithmetic|arithmetic]] — no floats anywhere in the provable path.

the embedding is unique up to rotation and reflection. one anchor (axiom 1) fixes translation only; orienting a 3D embedding needs at least four non-coplanar anchors (three for a planar one). axiom 1 as stated is the bootstrap, not the orientation — a revised construction is an open item.

### self-calibration

Earth's circumference provides the scale factor. nodes on opposite sides of the planet have RTT bounded by:

```
max_rtt ≈ 2 × (circumference / 2) / c_fiber ≈ 200 ms
```

the MDS embedding self-calibrates to Earth's scale from canonical propagation speeds. no GPS, no external reference frame needed.

## verification

```
interface:
  location_prove(neuron: NeuronId, mesh: RTTMesh) → LocationProof
  location_verify(proof: LocationProof, mesh: RTTMesh) → Result<Position, LocationError>
```

a location proof consists of:
- N RTT measurements to distinct peers (committed via Merkle tree)
- a [[delay]] proof on each challenge-response (prevents pre-computation)
- medium declaration for each link
- MDS embedding coordinates
- geometric consistency score

verification checks:
1. RTT measurements are internally consistent (triangle inequality)
2. delay proofs valid (timing cannot be gamed)
3. MDS embedding stress is below threshold (measurements consistent with 3D geometry)
4. declared medium matches observed RTT bounds
5. Merkle commitment covers all measurements (no selective presentation)

## sybil resistance

physical: faking RTT consistency across a dense global mesh requires physical presence at the claimed locations. an attacker in one location cannot simultaneously produce consistent RTT measurements to peers distributed worldwide.

economic: relay fees proportional to 1/latency make geographic honesty a dominant strategy. lying about location means suboptimal routing fees.

## properties

| property | value |
|----------|-------|
| precision | city-level (~50-100 km) from RTT alone |
| trust | one honest observer (axiom 4) |
| setup | zero trusted institutions |
| proof size | O(N) RTT measurements + delay proofs |
| verification | O(N²) for MDS, O(N) for consistency checks |

## dependencies

- hemera: Merkle commitment of RTT measurements
- [[delay]] (this repo): VDF on challenge-response
- nebu: fixed-point field arithmetic for MDS (eigendecomposition over Goldilocks)

## usage in cyber

**relay pricing**: relay fees proportional to inverse latency. geographic honesty = maximum relay revenue.

**consensus geography**: nodes declare location. BBG_poly(locations) records proven positions. sharding by geographic proximity becomes verifiable.

**signal ordering**: delay + place give physical spacetime ordering — time (VDF) and space (location).

## open questions

- minimum mesh density for a reliable MDS embedding
- precision vs number of RTT measurements
- indoor/outdoor medium detection
- satellite links (high latency, variable path)
- integration with the BBG_poly(locations) evaluation dimension
- the adversarial model under free "pretending distance", and the anchor set that orients the embedding (audit items)

[[foculus]] · [[delay]] · [[structural sync]]
