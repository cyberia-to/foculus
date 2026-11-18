use super::*;
use crate::chain::CyberlinkRecord;
use crate::proof::PublicProof;
use zheng::execution::certificate;

fn signal(amounts: &[u64]) -> Signal {
    Signal {
        neuron: [1; 32],
        network: [0; 32],
        links: amounts
            .iter()
            .enumerate()
            .map(|(i, &amount)| CyberlinkRecord {
                neuron: [1; 32],
                from: [1; 32],
                to: [2 + i as u8; 32],
                token: [9; 32],
                amount,
                valence: 1,
                height: 0,
            })
            .collect(),
        delta_pi: vec![],
        box_moves: vec![],
        prev: [0; 32],
        step: 1,
        height: 0,
        proof: None,
    }
}

#[test]
fn prove_and_verify_pay() {
    let s = signal(&[100, 50]);
    let proof = prove_pay(&s).unwrap();
    assert!(verify_pay(&proof, &s));
    let st = PayStatement::of(&s).unwrap();
    assert_eq!((st.total_out, st.leg_count), (150, 2));
}

#[test]
fn proof_does_not_transfer_to_another_signal() {
    let proof = prove_pay(&signal(&[100, 50])).unwrap();
    assert!(!verify_pay(&proof, &signal(&[100, 51])));
    let mut other_recipient = signal(&[100, 50]);
    other_recipient.links[0].to = [0xEE; 32];
    assert!(!verify_pay(&proof, &other_recipient));
}

#[test]
fn tampered_certificate_is_rejected() {
    let s = signal(&[7, 9, 11]);
    let Proof::Public(body) = prove_pay(&s).unwrap() else { panic!("public profile") };
    for i in [0, 1, body.certificate.free.len() - 1] {
        let mut bad = body.clone();
        bad.certificate.free[i] = (bad.certificate.free[i] + 1) % nebu::field::P;
        assert!(!verify_pay(&Proof::Public(bad), &s), "flip {i}");
    }
}

/// Legs that conserve the total but are not the signal's legs: the legs
/// are pinned from the signal, so no certificate of them exists, and one
/// made for the other signal is refused.
#[test]
fn reshuffled_legs_with_the_same_total_are_rejected() {
    let s = signal(&[100, 50]);
    let relation = PayRelation::of(&s).unwrap();
    let lie = signal(&[120, 30]);
    let lying = PayRelation::of(&lie).unwrap();
    let witness = lying.witness().unwrap();
    assert!(certificate::certify(&relation.instance(), &witness, &relation.public()).is_err());
    let cert = certificate::certify(&lying.instance(), &witness, &lying.public()).unwrap();
    let forged = Proof::Public(PublicProof { statement: relation.statement(), certificate: cert });
    assert!(!verify_pay(&forged, &s));
}

/// A running sum that wraps the field: `s_0 = p − 1` "pays" −2 next to a
/// leg of 101 — the field sums still close to the total, the range rows
/// do not. A leg of p − 1 cannot even be stated (above the pay range).
#[test]
fn wrapping_sums_cannot_be_certified() {
    let s = signal(&[40, 60]);
    let relation = PayRelation::of(&s).unwrap();
    let mut w = relation.witness().unwrap();
    let p = nebu::field::P;
    w.z[leg_sum(0)] = F::new(p - 1);
    assert!(certificate::certify(&relation.instance(), &w, &relation.public()).is_err());
    assert_eq!(PayStatement::of(&signal(&[p - 1, 3])).unwrap_err(), PayProofError::Overflow);
}

#[test]
fn pay_is_closed_and_proves_in_profile_one() {
    let s = signal(&[7, 9, 11]);
    let relation = PayRelation::of(&s).unwrap();
    let key = proof::RelationKey::new(&relation).unwrap();
    let p = proof::prove_succinct(&key, &relation).unwrap();
    assert_eq!(p.profile(), proof::PROFILE_SUCCINCT);
    assert!(verify_pay(&p, &s));
    proof::verify_with_key(&key, &relation, &p).unwrap();
    // bound to the signal: another amount, another recipient
    assert!(!verify_pay(&p, &signal(&[7, 9, 12])));
    let mut other = signal(&[7, 9, 11]);
    other.links[2].to = [0xEE; 32];
    assert!(!verify_pay(&p, &other));
    // a key of another shape is refused
    let two = signal(&[1, 2]);
    assert_eq!(
        proof::prove_succinct(&key, &PayRelation::of(&two).unwrap()),
        Err(proof::ProofError::NotClosed)
    );
}

#[test]
fn tampered_profile_one_pay_is_rejected() {
    let s = signal(&[5, 6]);
    let relation = PayRelation::of(&s).unwrap();
    let key = proof::RelationKey::new(&relation).unwrap();
    let Proof::Succinct(body) = proof::prove_succinct(&key, &relation).unwrap() else {
        panic!("profile 1")
    };
    let one = nebu::Fp3::new(F::ONE, F::ZERO, F::ZERO);
    let mut bad = body.clone();
    bad.proof.witness_eval += one;
    assert!(!verify_pay(&Proof::Succinct(bad), &s));
    let mut bad = body.clone();
    bad.proof.matrix_evals[0] += one;
    assert!(!verify_pay(&Proof::Succinct(bad), &s));
    let mut bad = body.clone();
    bad.statement[0] ^= 1;
    assert!(!verify_pay(&Proof::Succinct(bad), &s));
    assert!(verify_pay(&Proof::Succinct(body), &s));
}

#[test]
fn oversized_and_empty_pays_are_refused() {
    assert_eq!(prove_pay(&signal(&[])).unwrap_err(), PayProofError::Empty);
    let big = signal(&[MAX_PAY_AMOUNT, 1]);
    assert_eq!(prove_pay(&big).unwrap_err(), PayProofError::Overflow);
    let honest = prove_pay(&signal(&[1, 1])).unwrap();
    assert!(!verify_pay(&honest, &big));
    assert!(prove_pay(&signal(&[MAX_PAY_AMOUNT])).is_ok());
}

#[test]
fn signal_proof_check_names_the_reason() {
    let mut s = signal(&[5, 6]);
    assert_eq!(check_signal_proof(&s), Ok(()), "no proof passes");
    s.proof = Some(prove_pay(&s).unwrap());
    assert_eq!(check_signal_proof(&s), Ok(()));
    let proof = s.proof.clone().unwrap();
    s.links[1].amount = 7;
    assert_eq!(
        check_signal_proof(&s),
        Err(PayProofError::Proof(proof::ProofError::WrongStatement))
    );
    let Proof::Public(mut body) = proof else { panic!("public profile") };
    s.links[1].amount = 6;
    body.certificate.free[0] ^= 1;
    s.proof = Some(Proof::Public(body));
    assert!(matches!(check_signal_proof(&s), Err(PayProofError::Proof(_))));
    let mut empty = signal(&[]);
    empty.proof = s.proof.clone();
    assert_eq!(check_signal_proof(&empty), Err(PayProofError::Empty));
    assert!(PayProofError::Empty.to_string().contains("without links"));
}
