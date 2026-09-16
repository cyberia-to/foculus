---
tags: foculus, audit, storage
date: 2026-09-12
---
# durable Signal codec validation

The [versioned codec contract](../specs/signal-codec.md) is implemented in
`src/signal_codec.rs` and its cursor/proof modules. Strict legacy parsing is
implemented separately in `src/frames/strict.rs`. Existing permissive callers
retain their behavior and must explicitly opt into the new APIs.

Validated with Rust 1.95.0 on macOS arm64:

- `cargo test --locked --test signal_codec --test legacy_frames_strict`: 14 pass
  (8 full-codec/proof tests and 6 strict legacy tests).
- `cargo test --locked --lib frames::`: 7 existing frame tests pass.
- `cargo clippy --locked --lib --test signal_codec --test legacy_frames_strict
  --no-deps`: succeeds; 36 existing Foculus warnings, none in the added codecs
  or integration tests.
- Scoped rustfmt and whitespace checks pass for authored files.

Tests retain every Signal and supported Proof field, verify a real Zheng pay
proof after encoding/decoding, cover all byte truncations of representative
signals and frames, malformed lengths/tags/residues, nested collection bounds,
the 16 MiB aggregate encoding limit, exact legacy frame spans, and both legacy
log limits. Legacy network/proof information was never recorded and cannot be
recovered; strict import returns SELF_NETWORK/None under its named profile.
Decoding establishes bounded canonical representation, not proof validity.
Other Lens Opening variants are explicitly unsupported by this proof profile.

The direct Lens dependency exposes ColumnQuery for constructing the retained
authenticated opening. Cargo also refreshed eight stale local-package versions
to their existing sibling manifests: BBG, Lens, Assayer, Brakedown, Ikat,
Porphyry, Nox and Zheng. No registry version or checksum changed relative to the
preexisting lock snapshot (SHA256
`b23b9885a2d1fe357e03188fcd9e8a0c19ea465df047df3da7ed99f3f9d833b3`).
The orchestration task owns review/commit of the lockfile delta.
