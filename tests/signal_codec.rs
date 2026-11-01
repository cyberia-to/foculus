use foculus::signal_codec::{self as codec, ErrorKind, decode_signal, encode_signal, proof_hash};
use foculus::{BoxMoveRecord, CyberlinkRecord, Proof, PublicProof, Signal};
use zheng::execution::Certificate;

fn sample(proof: Option<Proof>) -> Signal {
    Signal {
        neuron: [1; 32],
        network: [2; 32],
        prev: [3; 32],
        step: 5,
        height: 7,
        links: vec![CyberlinkRecord {
            neuron: [8; 32],
            from: [9; 32],
            to: [10; 32],
            token: [11; 32],
            amount: u64::MAX,
            valence: i8::MIN,
            height: 19,
        }],
        delta_pi: vec![([12; 32], 23), ([13; 32], 29)],
        box_moves: vec![
            BoxMoveRecord {
                nullifier: [14; 32],
                commitment: None,
            },
            BoxMoveRecord {
                nullifier: [15; 32],
                commitment: Some(([16; 32], 31)),
            },
        ],
        proof,
    }
}
fn empty() -> Signal {
    let mut signal = sample(None);
    signal.links.clear();
    signal.delta_pi.clear();
    signal.box_moves.clear();
    signal
}
fn proof() -> Proof {
    Proof::Public(PublicProof {
        statement: [17; 32],
        certificate: Certificate {
            free: vec![37, 0, nebu::field::P - 1, 53],
        },
    })
}
fn free_mut(signal: &mut Signal) -> &mut Vec<u64> {
    let Some(Proof::Public(body)) = signal.proof.as_mut() else {
        unreachable!()
    };
    &mut body.certificate.free
}
/// Offset of the proof option byte in an encoding of `signal`.
fn proof_flag(signal: &Signal) -> usize {
    let mut bare = signal.clone();
    bare.proof = None;
    encode_signal(&bare).unwrap().len() - 1
}

#[test]
fn v2_roundtrip_retains_every_signal_and_proof_field() {
    let original = sample(Some(proof()));
    let bytes = encode_signal(&original).unwrap();
    assert_eq!(bytes[codec::MAGIC.len()], codec::VERSION);
    let decoded = decode_signal(&bytes).unwrap();
    assert_eq!(decoded.neuron, original.neuron);
    assert_eq!(decoded.network, original.network);
    assert_eq!(decoded.prev, original.prev);
    assert_eq!((decoded.step, decoded.height), (5, 7));
    let link = &decoded.links[0];
    assert_eq!(
        (link.neuron, link.from, link.to, link.token),
        ([8; 32], [9; 32], [10; 32], [11; 32])
    );
    assert_eq!((link.amount, link.valence, link.height), (u64::MAX, i8::MIN, 19));
    assert_eq!(decoded.delta_pi, original.delta_pi);
    assert_eq!(decoded.box_moves[1].commitment, Some(([16; 32], 31)));
    assert_eq!(decoded.proof, original.proof);
    assert_eq!(encode_signal(&decoded).unwrap(), bytes);
    let absent = decode_signal(&encode_signal(&sample(None)).unwrap()).unwrap();
    assert!(absent.proof.is_none());
}

#[test]
fn v1_signals_without_a_proof_still_decode() {
    for original in [sample(None), empty()] {
        let mut v1 = encode_signal(&original).unwrap();
        v1[codec::MAGIC.len()] = codec::LEGACY_VERSION;
        let decoded = decode_signal(&v1).unwrap();
        assert_eq!(decoded.content_id(), original.content_id());
        assert!(decoded.proof.is_none());
        // re-encoding upgrades to version 2
        let v2 = encode_signal(&decoded).unwrap();
        assert_eq!(v2[codec::MAGIC.len()], codec::VERSION);
    }
}

#[test]
fn v1_signals_carrying_a_legacy_proof_are_rejected() {
    let signal = sample(None);
    let mut v1 = encode_signal(&signal).unwrap();
    v1[codec::MAGIC.len()] = codec::LEGACY_VERSION;
    let flag = proof_flag(&signal);
    v1[flag] = 1;
    v1.extend_from_slice(&[0u8; 64]); // any legacy body
    assert!(matches!(
        decode_signal(&v1),
        Err(codec::CodecError { kind: ErrorKind::LegacyProof, position }) if position == flag + 1
    ));
}

#[test]
fn every_truncation_and_trailing_byte_is_rejected() {
    for original in [sample(None), sample(Some(proof()))] {
        let bytes = encode_signal(&original).unwrap();
        for n in 0..bytes.len() {
            assert!(decode_signal(&bytes[..n]).is_err(), "accepted prefix {n}");
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(matches!(
            decode_signal(&trailing),
            Err(codec::CodecError { kind: ErrorKind::Trailing, .. })
        ));
    }
}

#[test]
fn malformed_versions_profiles_and_values_are_rejected() {
    let bytes = encode_signal(&empty()).unwrap();
    let mut version = bytes.clone();
    version[codec::MAGIC.len()] = 3;
    assert!(matches!(
        decode_signal(&version),
        Err(codec::CodecError { kind: ErrorKind::UnsupportedVersion(3), .. })
    ));
    let mut domain = bytes.clone();
    domain[0] ^= 1;
    assert!(decode_signal(&domain).is_err());
    let links = codec::MAGIC.len() + 1 + 112;
    let mut count = bytes.clone();
    count[links..links + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode_signal(&count),
        Err(codec::CodecError { kind: ErrorKind::Limit(_), .. })
    ));
    let mut flag = bytes;
    *flag.last_mut().unwrap() = 2;
    assert!(decode_signal(&flag).is_err());

    let with_proof = sample(Some(proof()));
    let encoded = encode_signal(&with_proof).unwrap();
    let profile = proof_flag(&with_proof) + 1;
    let values = profile + 1 + 32 + 4;
    let mut unknown = encoded.clone();
    unknown[profile] = 2;
    assert!(matches!(
        decode_signal(&unknown),
        Err(codec::CodecError { kind: ErrorKind::UnsupportedProof, .. })
    ));
    let mut residue = encoded.clone();
    residue[values..values + 8].copy_from_slice(&nebu::field::P.to_le_bytes());
    assert!(matches!(
        decode_signal(&residue),
        Err(codec::CodecError { kind: ErrorKind::Invalid("Goldilocks residue"), .. })
    ));
    let mut trailing_zero = encoded.clone();
    let last = trailing_zero.len() - 8;
    trailing_zero[last..].copy_from_slice(&0u64.to_le_bytes());
    assert!(matches!(
        decode_signal(&trailing_zero),
        Err(codec::CodecError { kind: ErrorKind::Invalid("certificate trailing zero"), .. })
    ));
    let mut huge = encoded;
    huge[values - 4..values].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode_signal(&huge),
        Err(codec::CodecError { kind: ErrorKind::Limit(_), .. })
    ));
}

#[test]
fn encoding_rejects_noncanonical_certificates_and_oversized_signals() {
    let mut signal = sample(Some(proof()));
    free_mut(&mut signal).push(0);
    assert!(encode_signal(&signal).is_err());
    assert!(proof_hash(&signal).is_err());
    let mut signal = sample(Some(proof()));
    free_mut(&mut signal)[0] = nebu::field::P;
    assert!(encode_signal(&signal).is_err());
    let mut signal = sample(Some(proof()));
    signal.links = vec![signal.links[0].clone(); codec::MAX_LINKS + 1];
    assert!(encode_signal(&signal).is_err());
    let mut signal = empty();
    signal.proof = Some(proof());
    *free_mut(&mut signal) = vec![1; codec::MAX_CERTIFICATE_VALUES + 1];
    assert!(encode_signal(&signal).is_err());
    assert!(matches!(
        decode_signal(&vec![0; codec::MAX_SIGNAL_BYTES + 1]),
        Err(codec::CodecError { kind: ErrorKind::Limit(_), .. })
    ));
}

#[test]
fn proof_identity_binds_proof_bytes_independently_of_signal_fields() {
    let mut signal = sample(None);
    assert_eq!(proof_hash(&signal).unwrap(), [0; 32]);
    signal.proof = Some(proof());
    let original = proof_hash(&signal).unwrap();
    assert_ne!(original, [0; 32]);
    signal.network[0] ^= 1;
    signal.step += 1;
    assert_eq!(proof_hash(&signal).unwrap(), original);
    free_mut(&mut signal)[0] = 100;
    assert_ne!(proof_hash(&signal).unwrap(), original);
}

#[test]
fn a_real_pay_proof_verifies_after_signal_roundtrip() {
    let mut signal = empty();
    signal.links = vec![
        CyberlinkRecord {
            neuron: [1; 32],
            from: [1; 32],
            to: [4; 32],
            token: [5; 32],
            amount: 70,
            valence: 1,
            height: 0,
        };
        2
    ];
    signal.proof = Some(foculus::prove_pay(&signal).unwrap());
    let original_hash = proof_hash(&signal).unwrap();
    let decoded = decode_signal(&encode_signal(&signal).unwrap()).unwrap();
    assert!(foculus::verify_pay(decoded.proof.as_ref().unwrap(), &decoded));
    assert_eq!(proof_hash(&decoded).unwrap(), original_hash);
    let mut other = decoded.clone();
    other.links[1].amount = 71;
    assert!(!foculus::verify_pay(decoded.proof.as_ref().unwrap(), &other));
}
