//! Canonical, bounded, full Signal encoding for durable storage.
pub(crate) mod cursor;
mod proof;
use crate::{BoxMoveRecord, CyberlinkRecord, Signal};
use cursor::{Reader, Writer};

pub const MAGIC: &[u8] = b"foculus/signal\0";
pub const VERSION: u8 = 1;
pub const MAX_SIGNAL_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_LINKS: usize = 16_384;
pub const MAX_DELTAS: usize = 65_536;
pub const MAX_BOX_MOVES: usize = 65_536;
pub use proof::{
    MAX_COEFFICIENTS, MAX_COLUMN_INDEX, MAX_COLUMNS, MAX_FIELD_BYTES, MAX_MATRIX_EVALS,
    MAX_MERKLE_PATH, MAX_ROUNDS,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecError {
    pub position: usize,
    pub kind: ErrorKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Truncated,
    Trailing,
    Invalid(&'static str),
    Limit(&'static str),
    UnsupportedVersion(u8),
    UnsupportedProof,
}
impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "signal codec at byte {}: {:?}", self.position, self.kind)
    }
}
impl std::error::Error for CodecError {}

/// Bind the retained authenticated proof independently from chain/content IDs.
pub fn proof_hash(signal: &Signal) -> Result<[u8; 32], CodecError> {
    let Some(proof) = &signal.proof else {
        return Ok([0; 32]);
    };
    let mut out = Writer::new();
    proof::encode(&mut out, proof)?;
    let mut hash = cyber_hemera::Hasher::new();
    hash.update(b"foculus/signal-proof/v1\0");
    hash.update(&out.bytes);
    Ok(*hash.finalize().as_bytes())
}

pub fn encode_signal(signal: &Signal) -> Result<Vec<u8>, CodecError> {
    let mut out = Writer::new();
    out.put(MAGIC)?;
    out.byte(VERSION)?;
    out.put(&signal.neuron)?;
    out.put(&signal.network)?;
    out.put(&signal.prev)?;
    out.u64(signal.step)?;
    out.u64(signal.height)?;
    out.count(signal.links.len(), MAX_LINKS)?;
    for link in &signal.links {
        out.put(&link.neuron)?;
        out.put(&link.from)?;
        out.put(&link.to)?;
        out.put(&link.token)?;
        out.u64(link.amount)?;
        out.byte(link.valence as u8)?;
        out.u64(link.height)?;
    }
    out.count(signal.delta_pi.len(), MAX_DELTAS)?;
    for (particle, value) in &signal.delta_pi {
        out.put(particle)?;
        out.u64(*value)?;
    }
    out.count(signal.box_moves.len(), MAX_BOX_MOVES)?;
    for movement in &signal.box_moves {
        out.put(&movement.nullifier)?;
        out.byte(u8::from(movement.commitment.is_some()))?;
        if let Some((particle, value)) = movement.commitment {
            out.put(&particle)?;
            out.u64(value)?;
        }
    }
    out.byte(u8::from(signal.proof.is_some()))?;
    if let Some(proof) = &signal.proof {
        proof::encode(&mut out, proof)?;
    }
    Ok(out.bytes)
}

pub fn decode_signal(bytes: &[u8]) -> Result<Signal, CodecError> {
    let mut input = Reader::new(bytes, 0);
    if bytes.len() > MAX_SIGNAL_BYTES {
        return Err(input.error(ErrorKind::Limit("signal bytes")));
    }
    if input.take(MAGIC.len())? != MAGIC {
        return Err(input.error(ErrorKind::Invalid("signal domain")));
    }
    let version = input.byte()?;
    if version != VERSION {
        return Err(input.error(ErrorKind::UnsupportedVersion(version)));
    }
    let neuron = input.array()?;
    let network = input.array()?;
    let prev = input.array()?;
    let step = input.u64()?;
    let height = input.u64()?;
    let links = links(&mut input)?;
    let delta_pi = deltas(&mut input)?;
    let box_moves = boxes(&mut input)?;
    let proof = if input.flag()? {
        Some(proof::decode(&mut input)?)
    } else {
        None
    };
    input.finish()?;
    Ok(Signal {
        neuron,
        network,
        links,
        delta_pi,
        box_moves,
        prev,
        step,
        height,
        proof,
    })
}

pub(crate) fn links(input: &mut Reader<'_>) -> Result<Vec<CyberlinkRecord>, CodecError> {
    let count = input.count(MAX_LINKS, 145)?;
    let mut result = input.vector(count)?;
    for _ in 0..count {
        result.push(CyberlinkRecord {
            neuron: input.array()?,
            from: input.array()?,
            to: input.array()?,
            token: input.array()?,
            amount: input.u64()?,
            valence: input.byte()? as i8,
            height: input.u64()?,
        });
    }
    Ok(result)
}
pub(crate) fn deltas(input: &mut Reader<'_>) -> Result<Vec<([u8; 32], u64)>, CodecError> {
    let count = input.count(MAX_DELTAS, 40)?;
    let mut result = input.vector(count)?;
    for _ in 0..count {
        result.push((input.array()?, input.u64()?));
    }
    Ok(result)
}
pub(crate) fn boxes(input: &mut Reader<'_>) -> Result<Vec<BoxMoveRecord>, CodecError> {
    let count = input.count(MAX_BOX_MOVES, 33)?;
    let mut result = input.vector(count)?;
    for _ in 0..count {
        let nullifier = input.array()?;
        let commitment = if input.flag()? {
            Some((input.array()?, input.u64()?))
        } else {
            None
        };
        result.push(BoxMoveRecord {
            nullifier,
            commitment,
        });
    }
    Ok(result)
}
