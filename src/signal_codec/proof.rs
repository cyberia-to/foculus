//! Proof body of signal codec version 2: one profile byte, then the body.
//!
//! Profile 1 (public certificate v3): statement[32], u32 count, count
//! canonical Goldilocks u64 values, the last one nonzero. Version-1 bodies
//! (legacy HyperNova proofs) are not decoded: they proved nothing.
use super::cursor::{Reader, Writer};
use super::{CodecError, ErrorKind};
use crate::proof::{PROFILE_PUBLIC, Proof, PublicProof};
use nebu::field::P;
use zheng::execution::Certificate;

/// Most free certificate values one proof may carry (zheng's dimension cap).
pub const MAX_CERTIFICATE_VALUES: usize = 1 << 20;

pub(super) fn encode(out: &mut Writer, proof: &Proof) -> Result<(), CodecError> {
    match proof {
        Proof::Public(p) => {
            let free = &p.certificate.free;
            if free.iter().any(|&v| v >= P) || free.last() == Some(&0) {
                return Err(out.error(ErrorKind::Invalid("certificate value")));
            }
            out.byte(PROFILE_PUBLIC)?;
            out.put(&p.statement)?;
            out.count(free.len(), MAX_CERTIFICATE_VALUES)?;
            for &v in free {
                out.u64(v)?;
            }
            Ok(())
        }
        // Signals carry the pay σ, chosen to stay in the public profile
        // (audit/device-profile-choice-2026-10.md); a profile-1 proof on a
        // signal is refused rather than written in a form no decoder reads.
        Proof::Succinct(_) => Err(out.error(ErrorKind::UnsupportedProof)),
    }
}

pub(super) fn decode(input: &mut Reader<'_>) -> Result<Proof, CodecError> {
    let profile = input.byte()?;
    if profile != PROFILE_PUBLIC {
        return Err(input.error(ErrorKind::UnsupportedProof));
    }
    let statement = input.array()?;
    let count = input.count(MAX_CERTIFICATE_VALUES, 8)?;
    let mut free = input.vector(count)?;
    for _ in 0..count {
        let v = input.u64()?;
        if v >= P {
            return Err(input.error(ErrorKind::Invalid("Goldilocks residue")));
        }
        free.push(v);
    }
    if free.last() == Some(&0) {
        return Err(input.error(ErrorKind::Invalid("certificate trailing zero")));
    }
    Ok(Proof::Public(PublicProof {
        statement,
        certificate: Certificate { free },
    }))
}
