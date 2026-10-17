---
tags: foculus, protocol, storage
crystal-type: spec
---
# durable signal encoding

`signal_codec::encode_signal` and `decode_signal` encode the complete supported
Signal value. Version 2 starts with ASCII `foculus/signal\0` and byte `2`.
Integers use little endian; counts use u32. The payload order is neuron[32],
network[32], prev[32], step(u64), height(u64), links, delta_pi, box_moves, proof.
Each collection starts with its count; collection order is preserved.
Links contain neuron/from/to/token[32], amount(u64), valence(i8), height(u64).
Delta entries contain particle[32], amount(u64). Box moves contain nullifier[32]
and an option byte: 0, or 1 followed by commitment[32] and amount(u64).

Proof starts with an option byte. A present proof is a `crate::proof::Proof`:
one profile byte, then its body. Profile 1 is zheng's public certificate v3:
statement[32] (the digest of the statement the proof is bound to), a u32 count
and that many canonical Goldilocks u64 residues below 0xffffffff00000001, the
last one nonzero (trailing zeros are trimmed by the certifier, so the encoding
is canonical). Other profile bytes return UnsupportedProof. The codec preserves
the proof exactly; proof verification remains the caller's responsibility
(`verify_pay`, `verify_ticket`).

Version 1 is still decoded when its proof option byte is 0, yielding the same
Signal; encoding always writes version 2. A version-1 signal whose option byte
is 1 carried a legacy HyperNova proof, which proved nothing (zheng
`specs/decider.md`); decoding returns LegacyProof at the byte after the flag.

Limits are part of this version: 16 MiB total signal bytes, 16384 links, 65536
delta entries or box moves, and 2^20 certificate values per proof. Encoding
enforces the same limits as decoding. Counts are checked against both limits
and remaining bytes before allocation. Unsupported versions, noncanonical
tags/fields, truncation, trailing bytes and oversized values return positional
CodecError results. This format does not change Signal::hash or
Signal::content_id.
`signal_codec::proof_hash` returns zero[32] for an absent proof. A retained proof
returns Hemera(`foculus/signal-proof/v2\0` || canonical present-proof bytes),
using exactly the proof body above, without the option tag. It is independent
of the other Signal fields and rejects noncanonical or oversized proof bodies.

# strict legacy log import

`frames::decode_events_strict` reads the old concatenated tape log and returns
every Signal/Intent in order or one CodecError with its absolute byte position.
It requires the exact marker 0x1f, signal `!` or intent `^`, renderer `b`, minimal
unsigned LEB128 length, and an exactly consumed payload. It never resynchronizes
past bad bytes or skips an unknown event. Limits: 64 MiB log, 65536 events,
16 MiB frame payload; signal collection limits match versions 1 and 2.
`decode_events_strict_with_spans` also returns each frame's complete original
byte range, including its tape header, for byte-exact legacy import.

The legacy signal profile contains neuron, step, prev, height and counted links.
It permits exactly three historical suffix layouts: no suffix; counted box
moves; counted box moves followed by counted delta_pi. A partially present
suffix or any extra bytes are errors. Network and proof were absent from every
legacy layout: import explicitly reconstructs SELF_NETWORK and None. Legacy
intent payloads are exactly 136 bytes: neuron[32], h0(u64), scope_hash[32],
signature[64]. Legacy data cannot recover fields that its writer omitted.
Existing permissive decoding entry points retain their compatibility behavior;
durable acceptance and import must opt into the strict or full versioned API.
