use cyber_hemera::{Hash, Side};
use foculus::signal_codec::{self as codec, ErrorKind, decode_signal, encode_signal, proof_hash};
use foculus::{BoxMoveRecord, CyberlinkRecord, Signal};
use lens::{ColumnQuery, Commitment, Opening};
use nebu::Goldilocks;
use zheng::{Proof, SumcheckPoly};

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
    Proof {
        commitment: Commitment(Hash::from_bytes([17; 32])),
        matrix_evals: vec![Goldilocks::new(37), Goldilocks::new(nebu::field::P - 1)],
        outer_sumcheck_polys: vec![SumcheckPoly {
            degree: 1,
            coeffs: vec![Goldilocks::new(41), Goldilocks::new(43)],
        }],
        sumcheck_polys: vec![SumcheckPoly {
            degree: 0,
            coeffs: vec![Goldilocks::new(47)],
        }],
        eval_value: Goldilocks::new(53),
        pcs_opening: Opening::TensorMerkle {
            row_combination: 59u64.to_le_bytes().to_vec(),
            columns: vec![ColumnQuery {
                index: 61,
                column: 67u64.to_le_bytes().to_vec(),
                path: vec![
                    (Hash::from_bytes([18; 32]), Side::Left),
                    (Hash::from_bytes([19; 32]), Side::Right),
                ],
            }],
        },
    }
}

#[test]
fn roundtrip_retains_every_signal_and_authenticated_proof_field() {
    let original = sample(Some(proof()));
    let bytes = encode_signal(&original).unwrap();
    let decoded = decode_signal(&bytes).unwrap();
    assert_eq!(decoded.neuron, original.neuron);
    assert_eq!(decoded.network, original.network);
    assert_eq!(decoded.prev, original.prev);
    assert_eq!(
        (decoded.step, decoded.height),
        (original.step, original.height)
    );
    assert_eq!(decoded.links.len(), 1);
    let link = &decoded.links[0];
    assert_eq!(
        (link.neuron, link.from, link.to, link.token),
        ([8; 32], [9; 32], [10; 32], [11; 32])
    );
    assert_eq!(
        (link.amount, link.valence, link.height),
        (u64::MAX, i8::MIN, 19)
    );
    assert_eq!(decoded.delta_pi, original.delta_pi);
    assert_eq!(decoded.box_moves.len(), 2);
    assert_eq!(decoded.box_moves[0].nullifier, [14; 32]);
    assert_eq!(decoded.box_moves[0].commitment, None);
    assert_eq!(decoded.box_moves[1].nullifier, [15; 32]);
    assert_eq!(decoded.box_moves[1].commitment, Some(([16; 32], 31)));
    let before = original.proof.as_ref().unwrap();
    let after = decoded.proof.as_ref().unwrap();
    assert_eq!(after.commitment, before.commitment);
    assert_eq!(after.eval_value, before.eval_value);
    assert_eq!(after.matrix_evals, before.matrix_evals);
    assert_eq!(after.pcs_opening, before.pcs_opening);
    for (a, b) in [&after.outer_sumcheck_polys, &after.sumcheck_polys]
        .into_iter()
        .zip([&before.outer_sumcheck_polys, &before.sumcheck_polys])
    {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.degree, b.degree);
            assert_eq!(a.coeffs, b.coeffs);
        }
    }
    assert_eq!(encode_signal(&decoded).unwrap(), bytes);
    let absent = decode_signal(&encode_signal(&sample(None)).unwrap()).unwrap();
    assert!(absent.proof.is_none());
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
            Err(codec::CodecError {
                kind: ErrorKind::Trailing,
                ..
            })
        ));
    }
}

#[test]
fn malformed_versions_tags_counts_and_residues_are_rejected() {
    let bytes = encode_signal(&empty()).unwrap();
    let mut version = bytes.clone();
    version[codec::MAGIC.len()] = 2;
    assert!(matches!(
        decode_signal(&version),
        Err(codec::CodecError {
            kind: ErrorKind::UnsupportedVersion(2),
            ..
        })
    ));
    let mut domain = bytes.clone();
    domain[0] ^= 1;
    assert!(decode_signal(&domain).is_err());
    let links = codec::MAGIC.len() + 1 + 112;
    let mut count = bytes.clone();
    count[links..links + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode_signal(&count),
        Err(codec::CodecError {
            kind: ErrorKind::Limit(_),
            ..
        })
    ));
    count[links..links + 4].copy_from_slice(&(codec::MAX_LINKS as u32).to_le_bytes());
    assert!(matches!(
        decode_signal(&count),
        Err(codec::CodecError {
            kind: ErrorKind::Truncated,
            ..
        })
    ));
    let mut flag = bytes;
    *flag.last_mut().unwrap() = 2;
    assert!(decode_signal(&flag).is_err());
    let mut with_proof = empty();
    with_proof.proof = Some(proof());
    let mut encoded = encode_signal(&with_proof).unwrap();
    let scalar = links + 12 + 1 + 32;
    encoded[scalar..scalar + 8].copy_from_slice(&nebu::field::P.to_le_bytes());
    assert!(matches!(
        decode_signal(&encoded),
        Err(codec::CodecError {
            kind: ErrorKind::Invalid("Goldilocks residue"),
            ..
        })
    ));
    let mut encoded = encode_signal(&with_proof).unwrap();
    *encoded.last_mut().unwrap() = 2;
    assert!(matches!(
        decode_signal(&encoded),
        Err(codec::CodecError {
            kind: ErrorKind::Invalid("Merkle side"),
            ..
        })
    ));
}

#[test]
fn encoding_rejects_oversized_or_unsupported_proof_inputs() {
    let mut signal = sample(Some(proof()));
    signal.links = vec![signal.links[0].clone(); codec::MAX_LINKS + 1];
    assert!(encode_signal(&signal).is_err());
    signal = sample(Some(proof()));
    signal.proof.as_mut().unwrap().pcs_opening = Opening::Tensor {
        round_commitments: vec![],
        final_poly: vec![],
        query_responses: vec![],
    };
    assert!(matches!(
        encode_signal(&signal),
        Err(codec::CodecError {
            kind: ErrorKind::UnsupportedProof,
            ..
        })
    ));
    assert!(proof_hash(&signal).is_err());
    signal.proof = Some(proof());
    signal.proof.as_mut().unwrap().matrix_evals =
        vec![Goldilocks::ONE; codec::MAX_MATRIX_EVALS + 1];
    assert!(encode_signal(&signal).is_err());
    signal.proof = Some(proof());
    signal.proof.as_mut().unwrap().outer_sumcheck_polys[0]
        .coeffs
        .clear();
    assert!(encode_signal(&signal).is_err());
    signal.proof = Some(proof());
    let Opening::TensorMerkle {
        row_combination, ..
    } = &mut signal.proof.as_mut().unwrap().pcs_opening
    else {
        unreachable!()
    };
    *row_combination = nebu::field::P.to_le_bytes().to_vec();
    assert!(encode_signal(&signal).is_err());
    assert!(matches!(
        decode_signal(&vec![0; codec::MAX_SIGNAL_BYTES + 1]),
        Err(codec::CodecError {
            kind: ErrorKind::Limit(_),
            ..
        })
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
    signal.proof.as_mut().unwrap().matrix_evals[0] = Goldilocks::new(100);
    assert_ne!(proof_hash(&signal).unwrap(), original);
}

#[test]
fn a_real_zheng_pay_proof_verifies_after_signal_roundtrip() {
    let statement = foculus::PayStatement {
        content_id: [71; 32],
        total_out: 100,
        leg_count: 1,
    };
    let mut signal = sample(Some(foculus::prove_pay(&statement).unwrap()));
    let original_hash = proof_hash(&signal).unwrap();
    signal = decode_signal(&encode_signal(&signal).unwrap()).unwrap();
    assert!(foculus::verify_pay(
        signal.proof.as_ref().unwrap(),
        &statement
    ));
    assert_eq!(proof_hash(&signal).unwrap(), original_hash);
}

#[test]
fn every_nested_proof_count_and_field_encoding_is_bounded() {
    let body = encode_signal(&empty()).unwrap().len();
    let mut signal = empty();
    signal.proof = Some(proof());
    let original = encode_signal(&signal).unwrap();
    let matrix = body + 40;
    let outer = matrix + 4 + 16;
    let inner = outer + 4 + 1 + 4 + 16;
    let tag = inner + 4 + 1 + 4 + 8;
    let row = tag + 1;
    let columns = row + 4 + 8;
    let column_bytes = columns + 4 + 4;
    let path = column_bytes + 4 + 8;
    for offset in [
        matrix,
        outer,
        outer + 5,
        inner,
        inner + 5,
        row,
        columns,
        column_bytes,
        path,
    ] {
        let mut malformed = original.clone();
        malformed[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(
            matches!(
                decode_signal(&malformed),
                Err(codec::CodecError {
                    kind: ErrorKind::Limit(_),
                    ..
                })
            ),
            "count offset {offset}"
        );
    }
    for offset in [row + 4, column_bytes + 4, outer + 9, inner + 9] {
        let mut malformed = original.clone();
        malformed[offset..offset + 8].copy_from_slice(&nebu::field::P.to_le_bytes());
        assert!(
            decode_signal(&malformed).is_err(),
            "noncanonical field at {offset}"
        );
    }
    let mut malformed = original.clone();
    malformed[tag] = 0;
    assert!(matches!(
        decode_signal(&malformed),
        Err(codec::CodecError {
            kind: ErrorKind::UnsupportedProof,
            ..
        })
    ));
    let mut malformed = original;
    malformed[row..row + 4].copy_from_slice(&7u32.to_le_bytes());
    assert!(matches!(
        decode_signal(&malformed),
        Err(codec::CodecError {
            kind: ErrorKind::Invalid("field vector"),
            ..
        })
    ));
}

#[test]
fn encoder_enforces_each_collection_limit_and_aggregate_byte_budget() {
    let mut signal = empty();
    signal.delta_pi = vec![([0; 32], 0); codec::MAX_DELTAS + 1];
    assert!(encode_signal(&signal).is_err());
    signal = empty();
    signal.box_moves = vec![
        BoxMoveRecord {
            nullifier: [0; 32],
            commitment: None
        };
        codec::MAX_BOX_MOVES + 1
    ];
    assert!(encode_signal(&signal).is_err());
    for case in 0..6 {
        let mut candidate = proof();
        match case {
            0 => {
                candidate.outer_sumcheck_polys =
                    vec![candidate.outer_sumcheck_polys[0].clone(); codec::MAX_ROUNDS + 1]
            }
            1 => {
                candidate.sumcheck_polys =
                    vec![candidate.sumcheck_polys[0].clone(); codec::MAX_ROUNDS + 1]
            }
            _ => {
                let Opening::TensorMerkle {
                    row_combination,
                    columns,
                } = &mut candidate.pcs_opening
                else {
                    unreachable!()
                };
                match case {
                    2 => *row_combination = vec![0; codec::MAX_FIELD_BYTES + 8],
                    3 => *columns = vec![columns[0].clone(); codec::MAX_COLUMNS + 1],
                    4 => {
                        columns[0].path = vec![
                            (Hash::from_bytes([0; 32]), Side::Left);
                            codec::MAX_MERKLE_PATH + 1
                        ]
                    }
                    5 => columns[0].column = vec![0; codec::MAX_FIELD_BYTES + 8],
                    _ => unreachable!(),
                }
            }
        }
        signal = empty();
        signal.proof = Some(candidate);
        assert!(encode_signal(&signal).is_err(), "collection case {case}");
    }
    let mut candidate = proof();
    let Opening::TensorMerkle { columns, .. } = &mut candidate.pcs_opening else {
        unreachable!()
    };
    columns[0].column = vec![0; codec::MAX_FIELD_BYTES];
    *columns = vec![columns[0].clone(); 17];
    signal = empty();
    signal.proof = Some(candidate);
    assert!(matches!(
        encode_signal(&signal),
        Err(codec::CodecError {
            kind: ErrorKind::Limit("signal bytes"),
            ..
        })
    ));
}
