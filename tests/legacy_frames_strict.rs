use bbg::IntentRecord;
use foculus::frames::{
    MAX_LEGACY_EVENTS, MAX_LEGACY_LOG_BYTES, decode_events_strict, decode_events_strict_with_spans,
};
use foculus::signal_codec::{CodecError, ErrorKind, MAX_SIGNAL_BYTES};
use foculus::{BoxMoveRecord, CyberFrame, CyberlinkRecord, SELF_NETWORK, Signal};
use tade::{Chunk, MARKER, ReadResult, Reader, sigil};

fn signal() -> Signal {
    Signal {
        neuron: [1; 32],
        network: [9; 32],
        prev: [2; 32],
        step: 3,
        height: 4,
        links: vec![CyberlinkRecord {
            neuron: [5; 32],
            from: [6; 32],
            to: [7; 32],
            token: [8; 32],
            amount: 11,
            valence: -1,
            height: 13,
        }],
        delta_pi: vec![([14; 32], 17)],
        box_moves: vec![BoxMoveRecord {
            nullifier: [15; 32],
            commitment: Some(([16; 32], 19)),
        }],
        proof: None,
    }
}
fn payload(frame: &[u8]) -> Vec<u8> {
    let mut reader = Reader::new();
    reader.feed(frame);
    match reader.next_chunk() {
        ReadResult::Chunk(chunk) => chunk.payload.to_vec(),
        _ => panic!("fixture"),
    }
}
fn frame(sigil: u8, payload: &[u8]) -> Vec<u8> {
    Chunk::new(sigil, b'b', payload.to_vec().into()).encode()
}

#[test]
fn strict_import_preserves_order_spans_and_the_documented_legacy_profile() {
    let first = foculus::encode_signal_frame(&signal());
    let intent = IntentRecord {
        neuron: [20; 32],
        h0: 23,
        scope_hash: [21; 32],
        signature: [22; 64],
    };
    let second = foculus::encode_intent_frame(&intent);
    let log = [first.as_slice(), second.as_slice(), first.as_slice()].concat();
    let events = decode_events_strict_with_spans(&log).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].0, 0..first.len());
    assert_eq!(events[1].0, first.len()..first.len() + second.len());
    assert_eq!(events[2].0, first.len() + second.len()..log.len());
    assert_eq!(&log[events[1].0.clone()], second);
    let CyberFrame::Signal(decoded) = &events[0].1 else {
        panic!("signal")
    };
    assert_eq!(
        (decoded.neuron, decoded.prev, decoded.step, decoded.height),
        ([1; 32], [2; 32], 3, 4)
    );
    assert_eq!(decoded.network, SELF_NETWORK);
    assert!(decoded.proof.is_none());
    assert_eq!(decoded.links[0].token, [8; 32]);
    assert_eq!(decoded.links[0].valence, -1);
    assert_eq!(decoded.delta_pi, vec![([14; 32], 17)]);
    assert_eq!(decoded.box_moves[0].commitment, Some(([16; 32], 19)));
    let CyberFrame::Intent(decoded) = &events[1].1 else {
        panic!("intent")
    };
    assert_eq!(
        (
            decoded.neuron,
            decoded.h0,
            decoded.scope_hash,
            decoded.signature
        ),
        (
            intent.neuron,
            intent.h0,
            intent.scope_hash,
            intent.signature
        )
    );
    assert_eq!(decode_events_strict(&log).unwrap().len(), 3);
}

#[test]
fn strict_import_accepts_only_complete_historical_suffix_layouts() {
    let mut empty = signal();
    empty.links.clear();
    empty.delta_pi.clear();
    empty.box_moves.clear();
    let full = payload(&foculus::encode_signal_frame(&empty));
    assert_eq!(full.len(), 92);
    for len in [84, 88, 92] {
        assert_eq!(
            decode_events_strict(&frame(sigil::ZAP, &full[..len]))
                .unwrap()
                .len(),
            1
        );
    }
    for len in [85, 86, 87, 89, 90, 91] {
        assert!(decode_events_strict(&frame(sigil::ZAP, &full[..len])).is_err());
    }
    let mut extra = full;
    extra.push(0);
    assert!(matches!(
        decode_events_strict(&frame(sigil::ZAP, &extra)),
        Err(CodecError {
            kind: ErrorKind::Trailing,
            ..
        })
    ));
}

#[test]
fn strict_import_rejects_every_partial_frame_and_intent_payload() {
    let intent = IntentRecord {
        neuron: [1; 32],
        h0: 0,
        scope_hash: [2; 32],
        signature: [3; 64],
    };
    for bytes in [
        foculus::encode_signal_frame(&signal()),
        foculus::encode_intent_frame(&intent),
    ] {
        for end in 1..bytes.len() {
            assert!(
                decode_events_strict(&bytes[..end]).is_err(),
                "accepted partial frame {end}"
            );
        }
    }
    assert!(decode_events_strict(&[]).unwrap().is_empty());
    let data = payload(&foculus::encode_intent_frame(&intent));
    for end in 0..data.len() {
        assert!(decode_events_strict(&frame(sigil::KET, &data[..end])).is_err());
    }
    let mut trailing = data;
    trailing.push(0);
    assert!(matches!(
        decode_events_strict(&frame(sigil::KET, &trailing)),
        Err(CodecError {
            kind: ErrorKind::Trailing,
            ..
        })
    ));
}

#[test]
fn unknown_events_noise_and_bad_varints_are_errors_with_absolute_positions() {
    let valid = foculus::encode_signal_frame(&signal());
    for suffix in [
        vec![0],
        frame(sigil::HAX, b"unknown"),
        vec![MARKER, sigil::ZAP, b't', 0],
        vec![MARKER, sigil::ZAP, b'b', 0x80, 0],
    ] {
        let bytes = [valid.as_slice(), suffix.as_slice()].concat();
        let error = decode_events_strict(&bytes).err().expect("must fail");
        assert!(error.position > valid.len());
        assert!(error.position <= bytes.len());
    }
    let mut overflow = vec![MARKER, sigil::ZAP, b'b'];
    overflow.extend_from_slice(&[0xff; 10]);
    assert!(matches!(
        decode_events_strict(&overflow),
        Err(CodecError {
            kind: ErrorKind::Invalid("LEB128 overflow"),
            ..
        })
    ));
    let mut too_large = vec![MARKER, sigil::ZAP, b'b'];
    tade::encode_varint(MAX_SIGNAL_BYTES as u64 + 1, &mut too_large);
    assert!(matches!(
        decode_events_strict(&too_large),
        Err(CodecError {
            kind: ErrorKind::Limit(_),
            ..
        })
    ));
}

#[test]
fn collection_counts_and_option_tags_are_checked_before_allocation() {
    let mut raw = payload(&foculus::encode_signal_frame(&signal()));
    raw[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode_events_strict(&frame(sigil::ZAP, &raw)),
        Err(CodecError {
            kind: ErrorKind::Limit(_),
            ..
        })
    ));
    let mut raw = payload(&foculus::encode_signal_frame(&signal()));
    // Fixed header + one145-byte link + boxcount + nullifier.
    raw[84 + 145 + 4 + 32] = 2;
    assert!(matches!(
        decode_events_strict(&frame(sigil::ZAP, &raw)),
        Err(CodecError {
            kind: ErrorKind::Invalid("option tag"),
            ..
        })
    ));
}

#[test]
fn legacy_log_byte_and_event_limits_are_enforced() {
    assert!(matches!(
        decode_events_strict(&vec![0; MAX_LEGACY_LOG_BYTES + 1]),
        Err(CodecError {
            kind: ErrorKind::Limit(_),
            ..
        })
    ));
    let mut empty = signal();
    empty.links.clear();
    empty.delta_pi.clear();
    empty.box_moves.clear();
    let one = foculus::encode_signal_frame(&empty);
    let mut many = Vec::with_capacity(one.len() * (MAX_LEGACY_EVENTS + 1));
    for _ in 0..=MAX_LEGACY_EVENTS {
        many.extend_from_slice(&one);
    }
    assert!(matches!(
        decode_events_strict(&many),
        Err(CodecError {
            kind: ErrorKind::Limit("legacy event count"),
            ..
        })
    ));
}
