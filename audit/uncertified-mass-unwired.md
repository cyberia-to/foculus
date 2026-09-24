# Φ_uncert is a manual parameter, never computed — row 8's real blocker

date: 2026-09-24 · revision: 177fad4 (origin/master) · property #8

## the claim in the registry

> no two nodes finalize conflicting state | foculus | open | network gate with
> conflicts and partition; PR open: foculus#8 — MinHash agrees across n-way
> conflicts

foculus#8 (reviewed, mergeable) proves the deterministic core: given the same
conflict group, every arrival order names the same winner
(`fork::MinHash::resolve`, exercised n-way in
`minhash_agrees_across_n_way_conflicts_and_many_arrival_orders`). Its own
Remains section names what is still missing: "two nodes with genuinely
different (partial) local views converge to the same winner once views
complete." This note locates exactly where that gap lives in code, so the
next slice does not have to re-derive it.

## the gate that is supposed to close it

`specs/protocol.md:99` (step 6) and `specs/security-at-scale.md` (theorems
L1, L2) specify the actual safety condition precisely: a particle finalizes
only once a neuron's uncertified φ*-mass within its domain D,
Φ_uncert^(D), is below

```
Φ_uncert^(D) < (1 − κ_D)·Δ_D / (C·(1 + κ'))
```

`security-at-scale.md:54` gives the exact definition: Φ_uncert^(D) is the sum,
over particles in D, of φ*_D-mass belonging to particles with at least one
*uncertified* source — a source is "certified" once its in-domain
contribution has a VEC P2 completeness proof (`specs/vec.md:37`, an NMT range
proof, binary per source: either a source's publication is proven complete or
it is not).

`src/finality.rs:131` implements the inequality exactly as specified —
`certified(uncert_mass, gap, kappa_d, c, kappa_prime)`, cross-multiplied to
stay in the field, five tests covering the arithmetic
(`finality.rs:227-262`). That half is done and correct.

## what is missing

`uncert_mass: Fx` is a parameter `certified`/`finalizes` receive from the
caller. Nothing in this crate computes it:

- `src/cli.rs:150` — the `finality` subcommand takes `--uncert`, default
  `"0"`: an operator types the number in by hand.
- `grep -rn "uncert" src/*.rs` outside `finality.rs`/`finality_evidence.rs`/
  `cli.rs` returns nothing — no function reads a `ConflictGroup`
  (`conflict.rs`), a `Reconciler`'s index (`reconcile.rs`), or an NMT
  completeness proof (`nmt.rs::CompletenessProof`, `verify_full`) and folds
  them into Φ_uncert^(D).
- `conflict.rs`'s `ConflictIndex` tracks which signals it has seen and which
  keys they conflict on; it has no notion of "sources" as a countable,
  per-source-certifiable set, and no per-particle map from "which sources
  are relevant to this particle's φ* in D" — the second input
  `security-at-scale.md:60` says the computation needs and calls out as its
  own open enumeration gap ("P2 proves 'everything source s published,'
  never 'these are all the sources'").

So the certification *gate* (the inequality) is implemented and tested; the
certification *measurement* it gates on does not exist. `finalizes()` today
can only be driven by a human-supplied guess, which is why no two-node or
partition test can exist yet — there is nothing in the crate for such a test
to call other than the CLI's manual number.

## why this is not attempted here

Two design decisions are open before Φ_uncert^(D) can be computed, and
guessing at either risks the failure mode this codebase's own review
practice exists to avoid — a check that *looks* like it certifies
completeness while actually measuring something else (the same reasoning
bbg#19 gave for not guessing at `verify_opening`'s limb-packing convention):

1. **the source set.** Φ_uncert^(D) sums over particles with an uncertified
   *relevant source*. Nothing today enumerates "the sources relevant to
   particle i's φ* in domain D" — `security-at-scale.md:60`'s own "a source
   the neuron has never heard of" gap is unresolved in spec, not just in
   code. A function that silently assumed `ConflictIndex`'s observed
   `Signal`s are the complete source set would hide exactly this known-open
   enumeration problem behind a passing-looking type signature.
2. **what "certified" means for a source in code.** `vec.md` specifies VEC
   P2 as an NMT range proof per source; `nmt.rs` has the completeness-proof
   machinery (`CompletenessProof`, `verify_full`) but nothing here binds an
   NMT proof to "this signal's originating source, within this domain."
   That binding is the other half of the missing piece.

## the next slice, once one of those two decisions is made

Once the source-enumeration question has an answer (even a conservative one,
e.g. "sources = neurons with at least one signal already observed in D,
enumeration completeness accepted as a separate open item"), the shape of
the work is: a function `uncertified_mass(domain: &Domain, index: &ConflictIndex,
certified_sources: &BTreeSet<Particle>) -> Fx` that sums `domain`'s φ*-mass
over particles whose relevant source(s) are not in `certified_sources`, then
a two-node integration test where node B has certified fewer sources than
node A and `finalizes()` returns `Pending` for B until it catches up. That
test is the actual "no two nodes finalize conflicting state" claim; nothing
before it is.

## verified

```
$ grep -rn "uncert" src/*.rs | grep -v "finality.rs\|finality_evidence.rs\|cli.rs"
(no output)
$ cargo check --tests
error: failed to load manifest for dependency `cyber-tape`
```

The `cyber-tape` failure is the pre-existing `tape` → `tade` path break this
checkout has independent of this change (rows 37/39, foculus#37, unmerged);
confirmed identical on plain `origin/master` before this branch's one commit,
which adds only this file. No source touched.
