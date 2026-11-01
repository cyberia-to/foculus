use super::*;

/// `x < 2^8` with x free and `y = x` pinned.
struct Byte {
    y: u64,
    claim: u64,
}

impl Relation for Byte {
    fn statement(&self) -> [u8; 32] {
        digest(b"test-byte", &[&self.y.to_le_bytes()])
    }
    fn instance(&self) -> CCSInstance {
        let mut cs = R1cs::new(11);
        cs.packs(2, &(3..11).collect::<Vec<_>>());
        cs.zero(vec![(1, F::ONE), (2, -F::ONE)]);
        cs.instance()
    }
    fn public(&self) -> Vec<(usize, F)> {
        vec![(1, F::new(self.y))]
    }
    fn check_witness(&self, z: &[F]) -> bool {
        z[2] == F::new(self.y)
    }
}

impl Witnessed for Byte {
    fn witness(&self) -> Option<CCSWitness> {
        let mut z = vec![F::ZERO; 64];
        z[0] = F::ONE;
        z[1] = F::new(self.y);
        z[2] = F::new(self.claim);
        for k in 0..8 {
            z[3 + k] = F::new((self.claim >> k) & 1);
        }
        Some(CCSWitness { z })
    }
}

#[test]
fn honest_relation_proves_and_verifies() {
    let r = Byte { y: 200, claim: 200 };
    let p = prove(&r).unwrap();
    assert_eq!(p.profile(), PROFILE_PUBLIC);
    verify(&r, &p).unwrap();
}

#[test]
fn out_of_range_value_cannot_be_proven() {
    assert_eq!(
        prove(&Byte { y: 300, claim: 300 }),
        Err(ProofError::Unsatisfied)
    );
}

#[test]
fn proof_for_another_statement_is_rejected() {
    let p = prove(&Byte { y: 7, claim: 7 }).unwrap();
    assert_eq!(
        verify(&Byte { y: 8, claim: 8 }, &p),
        Err(ProofError::WrongStatement)
    );
}

#[test]
fn every_single_certificate_flip_is_rejected() {
    let r = Byte { y: 0b1010_1101, claim: 0b1010_1101 };
    let p = prove(&r).unwrap();
    let Proof::Public(body) = &p;
    for i in 0..body.certificate.free.len() {
        let mut bad = body.clone();
        bad.certificate.free[i] = (bad.certificate.free[i] + 1) % P;
        assert!(verify(&r, &Proof::Public(bad)).is_err(), "flip {i}");
    }
    let mut longer = body.clone();
    longer.certificate.free.push(1);
    assert!(verify(&r, &Proof::Public(longer)).is_err());
}

#[test]
fn native_witness_check_runs_on_the_disclosed_witness() {
    struct Lying(Byte);
    impl Relation for Lying {
        fn statement(&self) -> [u8; 32] {
            self.0.statement()
        }
        fn instance(&self) -> CCSInstance {
            self.0.instance()
        }
        fn public(&self) -> Vec<(usize, F)> {
            self.0.public()
        }
        fn check_witness(&self, _: &[F]) -> bool {
            false
        }
    }
    let r = Byte { y: 9, claim: 9 };
    let p = prove(&r).unwrap();
    assert_eq!(verify(&Lying(r), &p), Err(ProofError::WitnessCheck));
}

#[test]
fn wire_size_counts_profile_statement_count_and_values() {
    let p = prove(&Byte { y: 255, claim: 255 }).unwrap();
    let Proof::Public(body) = &p;
    assert_eq!(p.wire_size(), 37 + 8 * body.certificate.free.len());
}
