# foculus

consensus by convergence. a [[particle]] is final when $\phi^*_i > \tau$

## specs

implementable, normative — what a builder implements against:

- [protocol.md](specs/protocol.md) — the protocol spec: network model, state, conflicts, fork choice, seven-step protocol, safety/liveness, performance. amended to fold in domain-local certification, the support-switching rule, and the conditional spectral-gap theorem — see `security-at-scale.md` for the derivations
- [parameters.md](specs/parameters.md) — every protocol constant in one place: tri-kernel weights, VDF delays, precision floor, reorg depth, epoch windows — derived, measured, or explicitly chosen
- [gossip.md](specs/gossip.md) — the propagation layer VEC P4 assumes: no message envelope, a signal *is* the gossip message; push for new signals, pull for VEC P2 completeness proofs. topology starts with explicit domain subscription (proven, gossipsub-style, via radio); graph-inferred peer affinity is named as a future direction, not the starting design — see the document's own opening for why
- [provable-consensus.md](specs/provable-consensus.md) — circuit spec: proving φ* in a [[zheng]] circuit, cost analysis, recursive composition
- [vec.md](specs/vec.md) — verified eventual consistency: six properties (P1-P6), CRDT safety, NMT completeness, DAS availability
- [beacon.md](specs/beacon.md) — epoch randomness beacon $b_E$: VDF over finalized signals; unpredictable, unbiasable, verifiable, live
- [fold-mining.md](specs/fold-mining.md) — second lottery: HyperNova fold tree aggregates settlement tickets into one O(1) accumulator per cluster; closes settlement liveness
- [security-at-scale.md](specs/security-at-scale.md) — the derivation record for `protocol.md`'s amendments: safety and liveness at $N \to 10^{15}$, localized to reward specification's ε-support domains (L1, L2), the support-switching rule and its proof (T1), the spectral gap as a conditional theorem (T2), corrected domain/shard composition (S4) and liveness (S5)

## docs/explanation

narrative, not normative:

- [overview.md](docs/explanation/overview.md) — what is foculus, consensus as equilibrium, finality as point of no return
- [convergence.md](docs/explanation/convergence.md) — convergence theory: fixed points, contraction, spectral gap, five worked examples
- [life-of-a-signal.md](docs/explanation/life-of-a-signal.md) — a signal's full lifecycle walked in real time: gossip and tri-kernel convergence to finality (~4s), then the epoch pipeline — beacon, settlement lottery, fold, mint — to a spendable Shapley share (~1-3min); includes who computes the beacon, a tuned-vs-conservative timing table, and three open scheduling tensions
- [interplanetary.md](docs/explanation/interplanetary.md) — why foculus survives light-minutes where voting protocols cannot: the pond metaphor, four consequences (no round-trip, per-planet domains, partition-safety, information-not-clock finality), an Earth↔Mars signal walk, and the honest scope of the bounded cross-domain latency cost

## roadmap

open problems, one file each — not phased milestones, specific unsolved questions with enough context to pick up. see [roadmap/README.md](roadmap/README.md) for the index.
