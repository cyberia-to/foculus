use super::cursor::{Reader, Writer};
use super::{CodecError, ErrorKind};
use cyber_hemera::{Hash, Side};
use lens::{ColumnQuery, Commitment, Opening};
use nebu::Goldilocks;
use zheng::{Proof, SumcheckPoly};

pub const MAX_MATRIX_EVALS: usize = 4096;
pub const MAX_ROUNDS: usize = 64;
pub const MAX_COEFFICIENTS: usize = 256;
pub const MAX_COLUMNS: usize = 4096;
pub const MAX_COLUMN_INDEX: usize = u32::MAX as usize;
pub const MAX_FIELD_BYTES: usize = 1024 * 1024;
pub const MAX_MERKLE_PATH: usize = 64;

pub(super) fn encode(out: &mut Writer, proof: &Proof) -> Result<(), CodecError> {
    let Opening::TensorMerkle {
        row_combination,
        columns,
    } = &proof.pcs_opening
    else {
        return Err(out.error(ErrorKind::UnsupportedProof));
    };
    out.put(proof.commitment.as_bytes())?;
    out.u64(proof.eval_value.as_u64())?;
    out.count(proof.matrix_evals.len(), MAX_MATRIX_EVALS)?;
    for value in &proof.matrix_evals {
        out.u64(value.as_u64())?;
    }
    encode_rounds(out, &proof.outer_sumcheck_polys)?;
    encode_rounds(out, &proof.sumcheck_polys)?;
    out.byte(1)?;
    encode_field_bytes(out, row_combination)?;
    out.count(columns.len(), MAX_COLUMNS)?;
    for column in columns {
        out.count(column.index, MAX_COLUMN_INDEX)?;
        encode_field_bytes(out, &column.column)?;
        out.count(column.path.len(), MAX_MERKLE_PATH)?;
        for (hash, side) in &column.path {
            out.put(hash.as_bytes())?;
            out.byte(match side {
                Side::Left => 0,
                Side::Right => 1,
            })?;
        }
    }
    Ok(())
}

pub(super) fn decode(input: &mut Reader<'_>) -> Result<Proof, CodecError> {
    let commitment = Commitment(Hash::from_bytes(input.array()?));
    let eval_value = field(input)?;
    let count = input.count(MAX_MATRIX_EVALS, 8)?;
    let mut matrix_evals = input.vector(count)?;
    for _ in 0..count {
        matrix_evals.push(field(input)?);
    }
    let outer_sumcheck_polys = decode_rounds(input)?;
    let sumcheck_polys = decode_rounds(input)?;
    if input.byte()? != 1 {
        return Err(input.error(ErrorKind::UnsupportedProof));
    }
    let row_combination = decode_field_bytes(input)?;
    let count = input.count(MAX_COLUMNS, 12)?;
    let mut columns = input.vector(count)?;
    for _ in 0..count {
        let index = input.u32()? as usize;
        let column = decode_field_bytes(input)?;
        let count = input.count(MAX_MERKLE_PATH, 33)?;
        let mut path = input.vector(count)?;
        for _ in 0..count {
            let hash = Hash::from_bytes(input.array()?);
            let side = match input.byte()? {
                0 => Side::Left,
                1 => Side::Right,
                _ => return Err(input.error(ErrorKind::Invalid("Merkle side"))),
            };
            path.push((hash, side));
        }
        columns.push(ColumnQuery {
            index,
            column,
            path,
        });
    }
    Ok(Proof {
        commitment,
        matrix_evals,
        outer_sumcheck_polys,
        sumcheck_polys,
        eval_value,
        pcs_opening: Opening::TensorMerkle {
            row_combination,
            columns,
        },
    })
}

fn encode_rounds(out: &mut Writer, rounds: &[SumcheckPoly]) -> Result<(), CodecError> {
    out.count(rounds.len(), MAX_ROUNDS)?;
    for round in rounds {
        if round.coeffs.len() != usize::from(round.degree) + 1 {
            return Err(out.error(ErrorKind::Invalid("sumcheck coefficient count")));
        }
        out.byte(round.degree)?;
        out.count(round.coeffs.len(), MAX_COEFFICIENTS)?;
        for value in &round.coeffs {
            out.u64(value.as_u64())?;
        }
    }
    Ok(())
}
fn decode_rounds(input: &mut Reader<'_>) -> Result<Vec<SumcheckPoly>, CodecError> {
    let count = input.count(MAX_ROUNDS, 13)?;
    let mut rounds = input.vector(count)?;
    for _ in 0..count {
        let degree = input.byte()?;
        let count = input.count(MAX_COEFFICIENTS, 8)?;
        if count != usize::from(degree) + 1 {
            return Err(input.error(ErrorKind::Invalid("sumcheck coefficient count")));
        }
        let mut coeffs = input.vector(count)?;
        for _ in 0..count {
            coeffs.push(field(input)?);
        }
        rounds.push(SumcheckPoly { degree, coeffs });
    }
    Ok(rounds)
}
fn field(input: &mut Reader<'_>) -> Result<Goldilocks, CodecError> {
    let value = input.u64()?;
    if value >= nebu::field::P {
        return Err(input.error(ErrorKind::Invalid("Goldilocks residue")));
    }
    Ok(Goldilocks::new(value))
}
fn canonical_fields(bytes: &[u8]) -> bool {
    bytes.len().is_multiple_of(8)
        && bytes.chunks_exact(8).all(|chunk| {
            let mut encoded = [0; 8];
            encoded.copy_from_slice(chunk);
            u64::from_le_bytes(encoded) < nebu::field::P
        })
}
fn encode_field_bytes(out: &mut Writer, bytes: &[u8]) -> Result<(), CodecError> {
    out.count(bytes.len(), MAX_FIELD_BYTES)?;
    if !canonical_fields(bytes) {
        return Err(out.error(ErrorKind::Invalid("field vector")));
    }
    out.put(bytes)
}
fn decode_field_bytes(input: &mut Reader<'_>) -> Result<Vec<u8>, CodecError> {
    let count = input.count(MAX_FIELD_BYTES, 1)?;
    let bytes = input.take(count)?;
    if !canonical_fields(bytes) {
        return Err(input.error(ErrorKind::Invalid("field vector")));
    }
    let mut result = input.vector(count)?;
    result.extend_from_slice(bytes);
    Ok(result)
}
