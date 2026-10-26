//! Fail-closed import of the explicitly lossy historical signal profile.
use super::{CyberFrame, RENDER_BIN};
use crate::signal_codec::cursor::Reader;
use crate::signal_codec::{self, CodecError, ErrorKind, MAX_SIGNAL_BYTES};
use crate::{SELF_NETWORK, Signal};
use bbg::IntentRecord;

pub const MAX_LEGACY_LOG_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_LEGACY_EVENTS: usize = 65_536;

pub fn decode_events_strict(bytes: &[u8]) -> Result<Vec<CyberFrame>, CodecError> {
    let events = decode_events_strict_with_spans(bytes)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(events.len())
        .map_err(|_| CodecError {
            position: 0,
            kind: ErrorKind::Limit("allocation"),
        })?;
    result.extend(events.into_iter().map(|(_, event)| event));
    Ok(result)
}

/// Byte spans include the complete tade frame, allowing byte-exact import.
pub fn decode_events_strict_with_spans(
    bytes: &[u8],
) -> Result<Vec<(std::ops::Range<usize>, CyberFrame)>, CodecError> {
    let mut input = Reader::new(bytes, 0);
    if bytes.len() > MAX_LEGACY_LOG_BYTES {
        return Err(input.error(ErrorKind::Limit("legacy log bytes")));
    }
    let mut events = Vec::new();
    while input.remaining() != 0 {
        let frame_start = input.position;
        if events.len() == MAX_LEGACY_EVENTS {
            return Err(input.error(ErrorKind::Limit("legacy event count")));
        }
        if input.byte()? != tade::MARKER {
            return Err(input.error(ErrorKind::Invalid("tade marker")));
        }
        let sigil = input.byte()?;
        if !matches!(sigil, tade::sigil::ZAP | tade::sigil::KET) {
            return Err(input.error(ErrorKind::Invalid("legacy event sigil")));
        }
        if input.byte()? != RENDER_BIN {
            return Err(input.error(ErrorKind::Invalid("legacy event renderer")));
        }
        let length = length(&mut input)?;
        if length > MAX_SIGNAL_BYTES as u64 {
            return Err(input.error(ErrorKind::Limit("legacy frame bytes")));
        }
        let start = input.position;
        let payload = input.take(length as usize)?;
        let mut frame = Reader::new(payload, start);
        let event = match sigil {
            tade::sigil::ZAP => CyberFrame::Signal(signal(&mut frame)?),
            tade::sigil::KET => CyberFrame::Intent(intent(&mut frame)?),
            _ => return Err(frame.error(ErrorKind::Invalid("legacy event sigil"))),
        };
        frame.finish()?;
        events
            .try_reserve(1)
            .map_err(|_| input.error(ErrorKind::Limit("allocation")))?;
        events.push((frame_start..input.position, event));
    }
    Ok(events)
}

fn length(input: &mut Reader<'_>) -> Result<u64, CodecError> {
    let mut value = 0;
    for index in 0..10 {
        let byte = input.byte()?;
        let low = byte & 0x7f;
        if index == 9 && byte > 1 {
            return Err(input.error(ErrorKind::Invalid("LEB128 overflow")));
        }
        value |= u64::from(low) << (7 * index);
        if byte & 0x80 == 0 {
            if index > 0 && low == 0 {
                return Err(input.error(ErrorKind::Invalid("nonminimal LEB128")));
            }
            return Ok(value);
        }
    }
    Err(input.error(ErrorKind::Invalid("LEB128 overflow")))
}

fn signal(input: &mut Reader<'_>) -> Result<Signal, CodecError> {
    let neuron = input.array()?;
    let step = input.u64()?;
    let prev = input.array()?;
    let height = input.u64()?;
    let links = signal_codec::links(input)?;
    let box_moves = if input.remaining() == 0 {
        Vec::new()
    } else {
        signal_codec::boxes(input)?
    };
    let delta_pi = if input.remaining() == 0 {
        Vec::new()
    } else {
        signal_codec::deltas(input)?
    };
    Ok(Signal {
        neuron,
        network: SELF_NETWORK,
        prev,
        step,
        height,
        links,
        box_moves,
        delta_pi,
        proof: None,
    })
}
fn intent(input: &mut Reader<'_>) -> Result<IntentRecord, CodecError> {
    Ok(IntentRecord {
        neuron: input.array()?,
        h0: input.u64()?,
        scope_hash: input.array()?,
        signature: input.array()?,
    })
}
