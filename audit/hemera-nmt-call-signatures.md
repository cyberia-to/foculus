---
tags: foculus, audit
crystal-type: audit
crystal-domain: cyber
---
# the hemera calls in `src/nmt.rs`, checked against hemera's source

2026-11-18. the review of foculus#60 asked for evidence behind the new hemera
calls in `src/nmt.rs`, because `cargo check` cannot run on this pull request
yet: `Cargo.toml` pins `cyber-lens = "0.2.0"` and the lens checkout is at
`0.1.2` (`~/cyber/lens/Cargo.toml:16`, default branch `11113c6`), so resolution
fails before a single crate compiles. that pin is the PR's declared blocker —
it resolves when DECIDE lens#14 reaches the default branch — and this receipt
does not pretend otherwise. what it does is check the three signatures by
reading hemera, so the compiler is not the only thing that can catch a wrong
call.

checked against `cyber-hemera 0.3.1`, `~/cyber/hemera/rs`, default branch at
`b58ae54` (bench files dirty; nothing under `rs/src/`). the `Cargo.toml` pin is
`cyber-hemera = { version = "0.3", path = "../hemera/rs" }`, which `0.3.1`
satisfies.

| call site | call | hemera declaration | verdict |
|---|---|---|---|
| `src/nmt.rs` `leaf_hash` | `hash_leaf(&leaf.data_hash, leaf.namespace, is_root)` — `(&[u8; 32], u64, bool)` | `pub fn hash_leaf(chunk: &[u8], counter: u64, is_root: bool) -> Hash` (`rs/src/tree.rs:40`) | matches; `&[u8; 32]` coerces to `&[u8]` at the call |
| `src/nmt.rs` `hash_pair` | `hash_node_nmt(&Hash, &Hash, u64, u64, bool)` | `pub fn hash_node_nmt(left: &Hash, right: &Hash, ns_min: u64, ns_max: u64, is_root: bool) -> Hash` (`rs/src/tree.rs:170`) | matches |
| `src/nmt.rs` `hash_pair` | `Hash::from_bytes(left.hash)` — `[u8; 32]` | `pub const fn from_bytes(bytes: [u8; OUTPUT_BYTES]) -> Self` (`rs/src/sponge.rs:29`), `OUTPUT_BYTES = 32` (`rs/src/params.rs:90`) | matches |
| `src/nmt.rs`, tests | `*h.as_bytes()` → `[u8; 32]` | `pub fn as_bytes(&self) -> &[u8; OUTPUT_BYTES]` (`rs/src/sponge.rs:34`) | matches |
| `src/nmt.rs` tests | `cyber_hemera::hash(data)` | `pub fn hash(input: &[u8]) -> Hash` (`rs/src/lib.rs:93`) | matches |

paths: `tree` is a public module (`rs/src/lib.rs:81`) and `Hash` is re-exported
from the crate root (`rs/src/lib.rs:89`), so `use cyber_hemera::tree::{hash_leaf,
hash_node_nmt}; use cyber_hemera::Hash;` resolves.

semantics, for the record: `hash_node_nmt` commits the bounds in the sponge
capacity — `state[12] = ns_min`, `state[13] = ns_max` (`rs/src/tree.rs:170-186`,
hemera spec §4.6.4) — which is the rule `src/nmt.rs` claims to borrow rather
than restate. a namespace is reduced mod the Goldilocks prime when it enters
the state, so namespaces are only injective below $p = 2^{64} - 2^{32} + 1$.

this is a reading of source, not a run. it does not stand in for the build
gate: `cargo check` and `cargo test` are still unrun on this branch, and the
gate on the build server is the receipt that counts.
