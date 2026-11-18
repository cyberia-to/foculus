---
tags: foculus, audit, rust, net, iroh
date: 2026-09-24
---
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
already used `MemoryLookup`, and a peer is normally dialed through a known
`EndpointAddr` (parsed from `peers.json` in `node.rs`, or added via
`add_peer_addr` in `radio_settle.rs`). Swapping the production path's
`MdnsAddressLookup` for an empty `MemoryLookup` — the same type already
exercised everywhere else — is therefore not an interim stopgap; it matches
how a peer is actually located here.

One call site was not covered by that: `node.rs`'s `connect_peer` also
accepted a *bare* `EndpointId` and built an addressless
`EndpointAddr::new(id)` from it. With mDNS gone, `RelayMode::Disabled` and
an empty address book, such an address can never resolve, and the failure
surfaced only as "failed to connect to peer". `parse_peer_addr`
(`src/node.rs`) now rejects a bare id outright, in `add_peer` as well as at
dial time, naming the full `EndpointAddr` JSON as what is required.

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

This transcript is the migration slice's own run, taken on 2026-09-24 in the
row-39 worktree named under `revision:` above — before the placement and
node_util work and before the `origin/master` merge (#83) that this branch
now carries. It is not a gate for the branch head; the head's gate is the
build server's, and the manifest's `bbg = "0.2"` does not resolve against the
sibling `bbg` 0.3.0 checked out beside this repository, so the run cannot be
reproduced on a desk without a matching sibling revision.

That block is not this branch's doing and is not one line: `bbg = "0.2"` and
`zheng = "0.3"` stand unchanged on `master` (`Cargo.toml:25-26`) against
sibling checkouts already at 0.3.0 and 0.4.0, so the gate is red on this
branch's base as well. Bumping both pins is its own change — foculus#18
(`launch/scan-foculus-clean-build`), closed unmerged in the 2026-09-23 triage
sweep over a registry-row reference, branch kept and reopenable by the owner.
Until that lands, no commit of this repository can be judged by the gate.

The `RUSTC_BOOTSTRAP=1` prefix on every command below is not a requirement of
this crate: it was carried over from a sibling repository's build recipe
(`erga`). Neither `foculus` nor any of its path dependencies (`nebu`,
`cyber-hemera`, `bbg`, `zheng`, `tade`, `tru`, `tok`) declares a nightly
feature gate, and the repository pins no toolchain. Read the prefix as noise
in the transcript, not as a step to repeat.

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
