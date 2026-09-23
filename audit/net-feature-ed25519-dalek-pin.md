# audit: `net` feature build break — root cause and fix path

date: 2026-09-23
revision: 177fad4 (origin/master)
command: `cargo check --features net` (after locally applying row 47/foculus#18's
unmerged tape→tade and bbg/zheng pin bumps, since the manifest does not load
without them; those bumps are not part of this PR)

## root cause

`foculus`'s `net` feature depends on `iroh = "0.96"` (crates.io, upstream
n0-computer, not this org's fork). `iroh` 0.96.1 and `iroh-base` 0.96.1 both
pin `ed25519-dalek = "=3.0.0-pre.1"` exactly. That pre-release has a real
compile bug in its own `pkcs8` feature:

```
error[E0277]: `?` couldn't convert the error to `ed25519::pkcs8::Error`
   --> ed25519-dalek-3.0.0-pre.1/src/signing.rs:714
error[E0308]: mismatched types (pkcs8::Error::KeyMalformed as fn vs value)
   --> ed25519-dalek-3.0.0-pre.1/src/signing.rs:717
```

`ed25519-dalek` 3.0.0 (stable) fixes both, but the exact `=3.0.0-pre.1`
requirement in iroh's own manifest is not satisfiable by cargo's normal
resolution, `[patch.crates-io]` (patches must come from a different source
than the one being patched), or `cargo update --precise` (confirmed: cargo
refuses, naming `iroh-base v0.96.1`'s exact pin as the blocker).

## fix path found

`iroh` 1.2.0 (current latest) resolves `ed25519-dalek` to the fixed `3.0.0`
directly — confirmed with `cargo tree --features net -i ed25519-dalek`
against a locally bumped `iroh = "1.2"`. That is the real fix, but it is not
a small pin bump: `iroh` 1.x removed/renamed the `address-lookup-mdns`
feature this crate's `net` feature depends on for `SettleRadio`/`SyncNode`
(`cargo` reports it does not exist on 1.2's feature list: `default,
fast-apple-datapath, metrics, platform-verifier, portmapper, qlog,
test-utils, tls-aws-lc-rs, tls-ring, unstable-custom-transports,
unstable-net-report`), and the 0.96→1.x jump is a major API surface change
upstream, not audited here for how much of `node.rs`'s `net`-gated code it
breaks beyond the feature flag itself.

## remains

a separate, larger slice: migrate `net`'s `iroh` dependency from 0.96 to
1.2, replace `address-lookup-mdns` with whatever 1.x's discovery API calls
it (or drop mdns discovery if no longer offered), and fix whatever else in
`node.rs`'s settle-gossip path no longer matches iroh 1.x's API. Until that
lands, `net`'s own tests (rows 4, 6-9, 21, 58) stay unverifiable by
`cargo test --features net`; default-feature tests are unaffected.

## why not fixed here

row 58/foculus#19 flagged this compile break but explicity left it "not
investigated further." this closes that investigation with the root cause
and confirms the fix (iroh 1.x) is a real but larger slice than 40 minutes:
it is a dependency major-version migration across `net`'s discovery API,
not a pin number.
