---
tags: foculus, radio, settle, gossip
crystal-type: explanation
crystal-domain: cyber
---

# settle radio

how multi-miner settlement gossip rides real [[radio]] (iroh QUIC).

## why not a virtual mesh only

the in-process `SettleMesh` proves monoid + fanout algebra. production needs the same frames on the wire: length-prefixed settle messages over QUIC, dialed by endpoint public key, with topic = `claims_root`.

## stack

```
EpochRunner / miner
    │ encode_settle_msg (FSET/1)
    ▼
SettleRadio::publish
    │ iroh Endpoint::connect(peer, foculus/settle/1)
    │ uni-stream + length-prefixed frame
    ▼
peer SettleProtocol (Router ALPN)
    │ decode_settle_msg → dedupe → inbox
    ▼
settler wait_self_accs → settle_with_peer_accs
```

## ALPN

`foculus/settle/1` — separate from blob sync (`foculus/0`) so settle traffic does not share chunk handlers.

## wire

see `foculus/src/wire.rs`:

| type | magic | body |
|---|---|---|
| ClaimAnnounce | FSET v1 ty=1 | topic, claim_id, neuron, belief, prediction, links… |
| SelfAcc | FSET v1 ty=2 | topic, miner, k, sum_m, seen, commitment |
| ReceiptHash | FSET v1 ty=3 | topic, receipt_hash, epoch |

frame = `u32 LE len ‖ body`.

## API

| type | role |
|---|---|
| `SettleRadio` | bind endpoint, peer book, publish/drain |
| `RadioSettleSession` | topic-scoped announce claim / SelfAcc / receipt |
| `SettleMesh` | in-process epidemic (tests) |

## CLI

```
foculus settle-net listen --port 4210
foculus settle-net demo --peer '<EndpointAddr JSON>' --want 1
```

listen prints endpoint id + JSON addr for peers to dial.

## tests

`radio_settle::tests` spin two/three endpoints on a shared `MemoryLookup` (no mDNS, no public relay) and assert SelfAcc delivery + full settle receipt.

## next (optional)

- also register `iroh-gossip` topic mesh for fanout beyond explicit peer books  
- dual-ALPN on the blob `SyncNode` router so one process serves chunks + settle  
- ticket file format (base32) matching iroh chat example  

---

see [[fold-mining]], [[foculus beacon]], [[specs/gossip]], radio docs `gossip.md`.
