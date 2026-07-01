# foculus

consensus by convergence. a [[particle]] is final when $\phi^*_i > \tau$

## reference

implementable specifications:

- [foculus.md](reference/foculus.md) — protocol spec: network model, state, conflicts, fork choice, 7-step protocol, safety/liveness proofs, performance
- [provable-consensus.md](reference/provable-consensus.md) — circuit spec: proving φ* in a [[zheng]] circuit, cost analysis, recursive composition
- [vec.md](reference/vec.md) — verified eventual consistency: six properties (P1-P6), CRDT safety, NMT completeness, DAS availability
- [beacon.md](reference/beacon.md) — epoch randomness beacon $b_E$: VDF over finalized signals; unpredictable, unbiasable, verifiable, live
- [fold-mining.md](reference/fold-mining.md) — second lottery: HyperNova fold tree aggregates settlement tickets into one O(1) accumulator per cluster; closes settlement liveness

## docs

explainers:

- [overview.md](docs/overview.md) — what is foculus, consensus as equilibrium, finality as point of no return
- [convergence.md](docs/convergence.md) — convergence theory: fixed points, contraction, spectral gap, five worked examples
