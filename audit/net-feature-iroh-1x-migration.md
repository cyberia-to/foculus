# net feature: iroh 0.96 → 1.2 migration

date: 2026-09-24
revision: worktree off origin/master a3c9a0c-range (row 39 pin bump applied in the same commit)
command: `RUSTC_BOOTSTRAP=1 cargo check --features net --tests` and `cargo test --features net --lib`

## what was blocking

`iroh` was pinned to `0.96` with `features = ["address-lookup-mdns"]`. That
version resolves `ed25519-dalek = "=3.0.0-pre.1"` through `iroh-base`, and
that pre-release has a real compile bug in its own `pkcs8` feature — a
pre-existing, previously undiagnosed break in `cargo check --features net`,
first traced by the sibling audit that this fix follows up on.

Bumping to `iroh = "1.2"` resolves `ed25519-dalek` to the fixed `3.0.0`, but
1.x dropped the `address-lookup-mdns` Cargo feature and the `MdnsAddressLookup`
type entirely — `swarm-discovery`-based local network discovery is gone from
the crate, not renamed.

## the fix

`SettleRadio::start` (`src/radio_settle.rs`) and `SyncNode::start`
(`src/node.rs`) both used `MdnsAddressLookup::builder()` only in their
"production" constructor; every test and the `start_memory` constructor
already used `MemoryLookup`, and every peer dial in both files already
resolves through a known `EndpointAddr` (parsed from `peers.json` in
`node.rs`, or added via `add_peer_addr` in `radio_settle.rs`) rather than
through address-lookup resolution of a bare `EndpointId`. Swapping the
production path's `MdnsAddressLookup` for an empty `MemoryLookup` — the same
type already exercised everywhere else — is therefore not an interim stopgap;
it matches how every call site in this codebase actually locates a peer today.

Also required: `Endpoint::builder()` takes a `Preset` argument in iroh 1.x
(`iroh::endpoint::presets::Minimal` — sets only the mandatory crypto
provider, leaving every other option to the caller, matching how this crate
already configured `relay_mode`/`secret_key`/`address_lookup` by hand); and
`SecretKey::generate()` dropped its `&mut Rng` argument.

## what this does not close

Automatic local-network peer discovery (what mDNS provided) has no
replacement in this codebase yet. Nothing in the current test suite or
node-startup path relies on it — peers are always dialed by a known address —
so this is not a regression, but a node operator who previously relied on
mDNS auto-discovery on a local network has nothing to fall back on until a
real discovery mechanism (iroh's `DnsAddressLookup`/pkarr presets, or a
cyber-specific peer-exchange over an already-connected topic) is chosen.

## verified

```
$ RUSTC_BOOTSTRAP=1 cargo check --features net
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.73s
$ RUSTC_BOOTSTRAP=1 cargo check --features net --tests
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.61s
$ RUSTC_BOOTSTRAP=1 cargo test --features net --lib
test result: ok. 146 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
$ RUSTC_BOOTSTRAP=1 cargo test --features net --lib radio_settle
test result: ok. 2 passed; 0 failed; 0 ignored; 144 filtered out
$ RUSTC_BOOTSTRAP=1 cargo check --tests   # default features, unaffected
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.56s
$ RUSTC_BOOTSTRAP=1 cargo test            # default features, unaffected
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
