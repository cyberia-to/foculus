// ---
// tags: sync, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Cyber-dialect tape frames for the sync layer.
//!
//! sync mints and decodes tape frames carrying sync-protocol particles:
//! signals, intents, and chunk requests/responses. The tape framing is
//! self-describing (marker + sigil + render + varint + payload); the cyber
//! dialect assigns meaning to specific (sigil, render) pairs.
//!
//! Frame catalog for the cyber dialect:
//!
//! | particle          | sigil  | render | payload                                |
//! |-------------------|--------|--------|----------------------------------------|
//! | signal            | ZAP `!` | b      | bincode-encoded Signal envelope        |
//! | intent            | KET `^` | b      | bincode-encoded IntentRecord           |
//! | chunk_request     | WUT `?` | b      | (peer_id, chunk_id) tuple              |
//! | chunk_response    | HAX `#` | b      | raw chunk bytes + inclusion proof      |
//!
//! Radio carries these frames; sync registers `on_frame` handlers to receive
//! them and route to chain/erasure as appropriate.

use bbg::IntentRecord;
use tape::{Chunk, ReadResult, Reader, sigil};

use crate::{CyberlinkRecord, SELF_NETWORK, Signal};

/// Type-tagged identifier for the renderer field; cyber sync uses 'b'
/// (binary / bincode payload) across all frame kinds.
pub const RENDER_BIN: u8 = b'b';

/// Encode a signal as a cyber-dialect tape frame.
///
/// Sigil = ZAP (`!`, effect/imperative) since a signal is a sealed action.
pub fn encode_signal_frame(signal: &Signal) -> Vec<u8> {
    let payload = serialize_signal(signal);
    Chunk::new(sigil::ZAP, RENDER_BIN, payload.into()).encode()
}

/// Encode an intent record as a cyber-dialect tape frame.
///
/// Sigil = KET (`^`, lift/abstract) since an intent declares before action.
pub fn encode_intent_frame(intent: &IntentRecord) -> Vec<u8> {
    let payload = serialize_intent(intent);
    Chunk::new(sigil::KET, RENDER_BIN, payload.into()).encode()
}

/// Decode a cyber-dialect signal frame back into a [`Signal`].
///
/// The inverse of [`encode_signal_frame`]: unwraps the tape chunk (must carry
/// the ZAP sigil) and parses the signal payload. Returns `None` if the bytes
/// aren't a complete ZAP frame or the payload is malformed.
///
/// The wire form omits the network tag (a peer only ships its own network's
/// signals) and the proof — so a decoded signal carries `network =
/// SELF_NETWORK`, an empty `delta_pi`, and `proof = None`. That's exactly the
/// shape [`Cybergraph::link`](../../cybergraph) rebuilds locally, so a gossiped
/// signal dedups against a locally-applied one via the SignalChain.
pub fn decode_signal_frame(bytes: &[u8]) -> Option<Signal> {
    let mut reader = Reader::new();
    reader.feed(bytes);
    match reader.next_chunk() {
        ReadResult::Chunk(chunk) if chunk.sigil == sigil::ZAP => deserialize_signal(&chunk.payload),
        _ => None,
    }
}

/// Decode every signal frame in a concatenated tape stream, in wire order.
///
/// Tape frames are self-delimiting, so a durable log or a gossiped batch is
/// just frames back-to-back. Non-signal frames (other sigils) are skipped.
/// Each returned signal feeds straight into
/// [`Cybergraph::link`](../../cybergraph), which dedups via the SignalChain —
/// so replaying a log or applying a peer's batch is idempotent.
pub fn decode_signals(bytes: &[u8]) -> Vec<Signal> {
    let mut reader = Reader::new();
    reader.feed(bytes);
    let mut out = Vec::new();
    loop {
        match reader.next_chunk() {
            ReadResult::Chunk(chunk) => {
                if chunk.sigil == sigil::ZAP {
                    if let Some(s) = deserialize_signal(&chunk.payload) {
                        out.push(s);
                    }
                }
            }
            ReadResult::Pending | ReadResult::Eof => break,
        }
    }
    out
}

/// One decoded cyber-dialect frame, in wire order.
pub enum CyberFrame {
    Signal(Signal),
    Intent(IntentRecord),
}

/// Decode every signal and intent frame in a concatenated tape stream, in wire
/// order. Used by `on_peer_frame` ingress and by durable-log replay; unknown
/// sigils (chunk requests/responses, or a consumer's own markers) are skipped.
pub fn decode_events(bytes: &[u8]) -> Vec<CyberFrame> {
    let mut reader = Reader::new();
    reader.feed(bytes);
    let mut out = Vec::new();
    loop {
        match reader.next_chunk() {
            ReadResult::Chunk(c) if c.sigil == sigil::ZAP => {
                if let Some(s) = deserialize_signal(&c.payload) {
                    out.push(CyberFrame::Signal(s));
                }
            }
            ReadResult::Chunk(c) if c.sigil == sigil::KET => {
                if let Some(i) = deserialize_intent(&c.payload) {
                    out.push(CyberFrame::Intent(i));
                }
            }
            ReadResult::Chunk(_) => {}
            ReadResult::Pending | ReadResult::Eof => break,
        }
    }
    out
}

// ── minimal binary serialization ─────────────────────────────────────────
// Hand-rolled to avoid pulling in bincode/serde for sync; cyber-dialect
// payloads are small and stable enough to write by hand.

fn serialize_signal(s: &Signal) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32 + 8 + 32 + 8 + 8);
    buf.extend_from_slice(&s.neuron);
    buf.extend_from_slice(&s.step.to_le_bytes());
    buf.extend_from_slice(&s.prev);
    buf.extend_from_slice(&s.height.to_le_bytes());
    buf.extend_from_slice(&(s.links.len() as u32).to_le_bytes());
    for l in &s.links {
        buf.extend_from_slice(&l.neuron);
        buf.extend_from_slice(&l.from);
        buf.extend_from_slice(&l.to);
        buf.extend_from_slice(&l.token);
        buf.extend_from_slice(&l.amount.to_le_bytes());
        buf.push(l.valence as u8);
        buf.extend_from_slice(&l.height.to_le_bytes());
    }
    // trailing box_moves (backward compatible: old frames have no trailing bytes)
    buf.extend_from_slice(&(s.box_moves.len() as u32).to_le_bytes());
    for mv in &s.box_moves {
        buf.extend_from_slice(&mv.nullifier);
        match mv.commitment {
            Some((pt, val)) => {
                buf.push(1);
                buf.extend_from_slice(&pt);
                buf.extend_from_slice(&val.to_le_bytes());
            }
            None => buf.push(0),
        }
    }
    // trailing delta_pi (same tolerance rule: absent in old frames = empty).
    // The pi ledger rides the SAME log as everything else — a payment that
    // does not survive replay is not a payment.
    buf.extend_from_slice(&(s.delta_pi.len() as u32).to_le_bytes());
    for (to, amount) in &s.delta_pi {
        buf.extend_from_slice(to);
        buf.extend_from_slice(&amount.to_le_bytes());
    }
    buf
}

/// Inverse of [`serialize_signal`]. A little bounded cursor over the payload;
/// any short read (truncated frame) yields `None` rather than panicking.
///
/// Every collection count read off the wire goes through [`take_count`]
/// first, same technique as this crate's other wire-count fixes (rows 103,
/// 109, 129): a count is only accepted if the buffer has enough remaining
/// bytes to actually hold that many entries, so a crafted or truncated frame
/// can no longer drive an oversized `Vec::with_capacity` or an unbounded
/// decode loop — it fails at the count, not partway through the entries.
fn deserialize_signal(buf: &[u8]) -> Option<Signal> {
    let mut p = 0usize;
    let neuron = take32(buf, &mut p)?;
    let step = take_u64(buf, &mut p)?;
    let prev = take32(buf, &mut p)?;
    let height = take_u64(buf, &mut p)?;
    let count = take_count(buf, &mut p, 145)?; // neuron+from+to+token(32×4) + amount(8) + valence(1) + height(8)
    let mut links = Vec::with_capacity(count);
    for _ in 0..count {
        links.push(CyberlinkRecord {
            neuron: take32(buf, &mut p)?,
            from: take32(buf, &mut p)?,
            to: take32(buf, &mut p)?,
            token: take32(buf, &mut p)?,
            amount: take_u64(buf, &mut p)?,
            valence: take_u8(buf, &mut p)? as i8,
            height: take_u64(buf, &mut p)?,
        });
    }
    let mut box_moves = Vec::new();
    if p < buf.len() {
        let count = take_count(buf, &mut p, 33)?; // nullifier(32) + flag(1), commitment's 40 bytes are additional
        for _ in 0..count {
            let nullifier = take32(buf, &mut p)?;
            let flag = take_u8(buf, &mut p)?;
            let commitment = if flag == 1 {
                Some((take32(buf, &mut p)?, take_u64(buf, &mut p)?))
            } else {
                None
            };
            box_moves.push(crate::BoxMoveRecord {
                nullifier,
                commitment,
            });
        }
    }
    let mut delta_pi = Vec::new();
    if p < buf.len() {
        let count = take_count(buf, &mut p, 40)?; // to(32) + amount(8)
        for _ in 0..count {
            let to = take32(buf, &mut p)?;
            let amount = take_u64(buf, &mut p)?;
            delta_pi.push((to, amount));
        }
    }
    Some(Signal {
        neuron,
        network: SELF_NETWORK,
        links,
        delta_pi,
        box_moves,
        prev,
        step,
        height,
        proof: None,
    })
}

fn take32(buf: &[u8], p: &mut usize) -> Option<[u8; 32]> {
    let slice = buf.get(*p..*p + 32)?;
    *p += 32;
    slice.try_into().ok()
}

fn take_u64(buf: &[u8], p: &mut usize) -> Option<u64> {
    let slice = buf.get(*p..*p + 8)?;
    *p += 8;
    Some(u64::from_le_bytes(slice.try_into().ok()?))
}

fn take_u32(buf: &[u8], p: &mut usize) -> Option<u32> {
    let slice = buf.get(*p..*p + 4)?;
    *p += 4;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

fn take_u8(buf: &[u8], p: &mut usize) -> Option<u8> {
    let b = *buf.get(*p)?;
    *p += 1;
    Some(b)
}

/// Read a wire-supplied collection count, rejecting it outright rather than
/// letting the caller preallocate or loop on an attacker-chosen `u32`: the
/// remaining buffer must actually be able to hold that many `min_bytes`-sized
/// entries. `min_bytes` is each entry's minimum encoded size (for a variant
/// with an optional trailing part, the size of its shortest form), so this
/// never rejects a count a well-formed buffer could still satisfy.
fn take_count(buf: &[u8], p: &mut usize, min_bytes: usize) -> Option<usize> {
    let count = take_u32(buf, p)? as usize;
    if count > buf.len().saturating_sub(*p) / min_bytes {
        return None;
    }
    Some(count)
}

fn serialize_intent(i: &IntentRecord) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32 + 8 + 32 + 64);
    buf.extend_from_slice(&i.neuron);
    buf.extend_from_slice(&i.h0.to_le_bytes());
    buf.extend_from_slice(&i.scope_hash);
    buf.extend_from_slice(&i.signature);
    buf
}

fn deserialize_intent(buf: &[u8]) -> Option<IntentRecord> {
    let mut p = 0usize;
    let neuron = take32(buf, &mut p)?;
    let h0 = take_u64(buf, &mut p)?;
    let scope_hash = take32(buf, &mut p)?;
    let signature: [u8; 64] = buf.get(p..p + 64)?.try_into().ok()?;
    Some(IntentRecord {
        neuron,
        h0,
        scope_hash,
        signature,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Signal;

    fn empty_signal() -> Signal {
        Signal {
            neuron: [1u8; 32],
            network: crate::SELF_NETWORK,
            links: vec![],
            delta_pi: vec![],
            box_moves: vec![],
            prev: [0u8; 32],
            step: 0,
            height: 0,
            proof: None,
        }
    }

    fn empty_intent() -> IntentRecord {
        IntentRecord {
            neuron: [1u8; 32],
            h0: 0,
            scope_hash: [2u8; 32],
            signature: [0u8; 64],
        }
    }

    #[test]
    fn signal_frame_starts_with_tape_marker_and_zap_sigil() {
        let frame = encode_signal_frame(&empty_signal());
        assert_eq!(frame[0], 0x1F, "tape unit-separator marker");
        assert_eq!(frame[1], sigil::ZAP, "signal sigil");
        assert_eq!(frame[2], RENDER_BIN);
    }

    #[test]
    fn intent_frame_starts_with_tape_marker_and_ket_sigil() {
        let frame = encode_intent_frame(&empty_intent());
        assert_eq!(frame[0], 0x1F);
        assert_eq!(frame[1], sigil::KET);
        assert_eq!(frame[2], RENDER_BIN);
    }

    #[test]
    fn signal_payload_length_matches_serialization() {
        let s = empty_signal();
        let payload = serialize_signal(&s);
        // 32 (neuron) + 8 (step) + 32 (prev) + 8 (height) + 4 (link_count)
        // + 4 (box_moves count) + 4 (delta_pi count)
        assert_eq!(payload.len(), 32 + 8 + 32 + 8 + 4 + 4 + 4);
    }

    #[test]
    fn signal_frame_roundtrips_through_decode() {
        let mut s = empty_signal();
        s.step = 7;
        s.height = 42;
        s.prev = [9u8; 32];
        s.links.push(CyberlinkRecord {
            neuron: [1u8; 32],
            from: [2u8; 32],
            to: [3u8; 32],
            token: [4u8; 32],
            amount: 5,
            valence: -1,
            height: 42,
        });
        let frame = encode_signal_frame(&s);
        let back = decode_signal_frame(&frame).expect("decodes");
        assert_eq!(back.neuron, s.neuron);
        assert_eq!(back.step, s.step);
        assert_eq!(back.prev, s.prev);
        assert_eq!(back.height, s.height);
        assert_eq!(back.links.len(), 1);
        assert_eq!(back.links[0].to, [3u8; 32]);
        assert_eq!(back.links[0].valence, -1);
        // decoded signal hashes identically to the original — the wire form is
        // faithful, so a gossiped signal dedups against a local one.
        assert_eq!(back.hash(), s.hash());
    }

    #[test]
    fn decode_rejects_non_signal_bytes() {
        assert!(decode_signal_frame(b"not a tape frame").is_none());
        assert!(decode_signal_frame(&[]).is_none());
    }

    #[test]
    fn intent_payload_length_is_fixed() {
        let i = empty_intent();
        let payload = serialize_intent(&i);
        // 32 + 8 + 32 + 64 = 136
        assert_eq!(payload.len(), 136);
    }

    // ── take_count: direct coverage of the two-part guard ──────────────────

    #[test]
    fn take_count_accepts_count_the_remaining_bytes_can_hold() {
        let mut buf = vec![0u8; 4 + 145];
        buf[0..4].copy_from_slice(&1u32.to_le_bytes());
        let mut p = 0usize;
        assert_eq!(take_count(&buf, &mut p, 145), Some(1));
        assert_eq!(p, 4);
    }

    #[test]
    fn take_count_rejects_when_remaining_bytes_cannot_hold_declared_entries() {
        let buf = 5u32.to_le_bytes(); // claims 5 entries, 0 bytes follow
        let mut p = 0usize;
        assert_eq!(take_count(&buf, &mut p, 145), None);
    }

    #[test]
    fn take_count_rejects_huge_count_on_a_short_buffer() {
        let buf = u32::MAX.to_le_bytes();
        let mut p = 0usize;
        // u32::MAX / 145 would previously reach Vec::with_capacity directly;
        // here it must fail at the count, before any allocation.
        assert_eq!(take_count(&buf, &mut p, 145), None);
    }

    // ── deserialize_signal: the crafted-frame regressions row 103 flagged ──
    // (an unbounded count let a crafted or truncated frame drive an
    // arbitrarily large Vec::with_capacity and an unbounded decode loop;
    // these confirm every collection count is now rejected at the count.)

    fn header_bytes() -> Vec<u8> {
        vec![0u8; 32 + 8 + 32 + 8] // neuron, step, prev, height
    }

    #[test]
    fn decode_rejects_huge_link_count_on_a_short_buffer() {
        let mut buf = header_bytes();
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(deserialize_signal(&buf).is_none());
    }

    #[test]
    fn decode_rejects_link_count_when_buffer_too_short_for_declared_entries() {
        let mut buf = header_bytes();
        buf.extend_from_slice(&1u32.to_le_bytes()); // claims one link, none follow
        assert!(deserialize_signal(&buf).is_none());
    }

    #[test]
    fn decode_rejects_huge_box_move_count_on_a_short_buffer() {
        let mut buf = header_bytes();
        buf.extend_from_slice(&0u32.to_le_bytes()); // zero links
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(deserialize_signal(&buf).is_none());
    }

    #[test]
    fn decode_rejects_huge_delta_pi_count_on_a_short_buffer() {
        let mut buf = header_bytes();
        buf.extend_from_slice(&0u32.to_le_bytes()); // zero links
        buf.extend_from_slice(&0u32.to_le_bytes()); // zero box_moves
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(deserialize_signal(&buf).is_none());
    }
}

#[cfg(test)]
mod delta_pi_frame_tests {
    use super::*;

    #[test]
    fn delta_pi_round_trips_and_old_frames_stay_readable() {
        let mut sig = Signal {
            neuron: [7u8; 32],
            network: SELF_NETWORK,
            links: vec![],
            delta_pi: vec![([9u8; 32], 40), ([11u8; 32], 2)],
            box_moves: vec![],
            prev: [0u8; 32],
            step: 1,
            height: 5,
            proof: None,
        };
        let bytes = encode_signal_frame(&sig);
        let back = decode_signal_frame(&bytes).expect("decode");
        assert_eq!(back.delta_pi, sig.delta_pi);

        // An old-style frame (no trailing sections) reads as empty.
        sig.delta_pi.clear();
        let bytes = encode_signal_frame(&sig);
        let back = decode_signal_frame(&bytes).expect("decode old-shape");
        assert!(back.delta_pi.is_empty());
    }
}
