// ---
// tags: foculus, rust, wire, radio, settle
// crystal-type: source
// crystal-domain: cyber
// ---
//! Binary wire codec for settle-gossip messages over radio.
//!
//! Frame: `FSET` ‖ version(u8) ‖ type(u8) ‖ payload…
//! Types: 1=ClaimAnnounce, 2=SelfAcc, 3=ReceiptHash
//!
//! Used by in-process mesh and by the iroh ALPN settle protocol.

use std::collections::BTreeSet;

use nebu::Goldilocks;
use tru::{Fx, Link};

use crate::gossip::{SettleMsg, Topic};
use crate::rewards::RewardClaim;
use crate::tickets::ClusterAcc;

const MAGIC: &[u8; 4] = b"FSET";
const VERSION: u8 = 1;

const TY_CLAIM: u8 = 1;
const TY_SELF_ACC: u8 = 2;
const TY_RECEIPT: u8 = 3;

/// Encode any settle message for radio transport.
pub fn encode_settle_msg(msg: &SettleMsg) -> Vec<u8> {
    match msg {
        SettleMsg::ClaimAnnounce {
            topic,
            claim_id,
            neuron,
            claim,
        } => {
            let mut out = header(TY_CLAIM);
            out.extend_from_slice(topic);
            out.extend_from_slice(claim_id);
            out.extend_from_slice(neuron);
            encode_claim_body(&mut out, claim);
            out
        }
        SettleMsg::SelfAcc { topic, miner, acc } => {
            let mut out = header(TY_SELF_ACC);
            out.extend_from_slice(topic);
            out.extend_from_slice(miner);
            encode_acc_body(&mut out, acc);
            out
        }
        SettleMsg::ReceiptHash {
            topic,
            receipt_hash,
            epoch,
        } => {
            let mut out = header(TY_RECEIPT);
            out.extend_from_slice(topic);
            out.extend_from_slice(receipt_hash);
            out.extend_from_slice(&epoch.to_le_bytes());
            out
        }
    }
}

/// Decode a settle message from wire bytes.
pub fn decode_settle_msg(bytes: &[u8]) -> Option<SettleMsg> {
    if bytes.len() < 6 {
        return None;
    }
    if &bytes[0..4] != MAGIC || bytes[4] != VERSION {
        return None;
    }
    let ty = bytes[5];
    let mut off = 6;
    match ty {
        TY_CLAIM => {
            let topic = read32(bytes, &mut off)?;
            let claim_id = read32(bytes, &mut off)?;
            let neuron = read32(bytes, &mut off)?;
            let claim = decode_claim_body(bytes, &mut off, claim_id, neuron)?;
            Some(SettleMsg::ClaimAnnounce {
                topic,
                claim_id,
                neuron,
                claim,
            })
        }
        TY_SELF_ACC => {
            let topic = read32(bytes, &mut off)?;
            let miner = read32(bytes, &mut off)?;
            let acc = decode_acc_body(bytes, &mut off)?;
            Some(SettleMsg::SelfAcc { topic, miner, acc })
        }
        TY_RECEIPT => {
            let topic = read32(bytes, &mut off)?;
            let receipt_hash = read32(bytes, &mut off)?;
            let epoch = read_u64(bytes, &mut off)?;
            Some(SettleMsg::ReceiptHash {
                topic,
                receipt_hash,
                epoch,
            })
        }
        _ => None,
    }
}

/// Length-prefixed frame for stream transport: u32 LE length + body.
pub fn encode_frame(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// Read one length-prefixed frame from a buffer; returns (frame, rest).
pub fn split_frame(buf: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    if buf.len() < 4 {
        return None;
    }
    let n = u32::from_le_bytes(buf[0..4].try_into().ok()?) as usize;
    if buf.len() < 4 + n {
        return None;
    }
    Some((buf[4..4 + n].to_vec(), &buf[4 + n..]))
}

fn header(ty: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(ty);
    out
}

fn encode_claim_body(out: &mut Vec<u8>, claim: &RewardClaim) {
    out.extend_from_slice(&claim.belief.raw().as_u64().to_le_bytes());
    out.extend_from_slice(&claim.prediction.raw().as_u64().to_le_bytes());
    out.extend_from_slice(&(claim.links.len() as u32).to_le_bytes());
    for l in &claim.links {
        encode_link(out, l);
    }
}

fn decode_claim_body(
    bytes: &[u8],
    off: &mut usize,
    id: [u8; 32],
    neuron: [u8; 32],
) -> Option<RewardClaim> {
    let belief = Fx::from_raw(Goldilocks::new(read_u64(bytes, off)?));
    let prediction = Fx::from_raw(Goldilocks::new(read_u64(bytes, off)?));
    let n = read_u32(bytes, off)? as usize;
    let mut links = Vec::with_capacity(n);
    for _ in 0..n {
        links.push(decode_link(bytes, off)?);
    }
    Some(RewardClaim {
        id,
        neuron,
        links,
        belief,
        prediction,
    })
}

fn encode_link(out: &mut Vec<u8>, l: &Link) {
    out.extend_from_slice(&l.neuron);
    out.extend_from_slice(&l.from);
    out.extend_from_slice(&l.to);
    out.extend_from_slice(&l.amount.to_le_bytes());
    out.extend_from_slice(&l.valence.to_le_bytes());
    out.extend_from_slice(&l.price.raw().as_u64().to_le_bytes());
}

fn decode_link(bytes: &[u8], off: &mut usize) -> Option<Link> {
    let neuron = read32(bytes, off)?;
    let from = read32(bytes, off)?;
    let to = read32(bytes, off)?;
    let amount = read_u128(bytes, off)?;
    if *off >= bytes.len() {
        return None;
    }
    let valence = bytes[*off] as i8;
    *off += 1;
    // valence is i8 but we wrote to_le_bytes which is 1 byte for i8... wait i8::to_le_bytes is 1 byte
    let price = Fx::from_raw(Goldilocks::new(read_u64(bytes, off)?));
    Some(Link {
        neuron,
        from,
        to,
        amount,
        valence,
        price,
    })
}

fn encode_acc_body(out: &mut Vec<u8>, acc: &ClusterAcc) {
    out.extend_from_slice(&acc.k.to_le_bytes());
    out.extend_from_slice(&(acc.sum_m.len() as u32).to_le_bytes());
    for m in &acc.sum_m {
        out.extend_from_slice(&m.raw().as_u64().to_le_bytes());
    }
    out.extend_from_slice(&(acc.seen.len() as u32).to_le_bytes());
    for (miner_id, nonce) in &acc.seen {
        out.extend_from_slice(miner_id);
        out.extend_from_slice(&nonce.to_le_bytes());
    }
    out.extend_from_slice(&acc.commitment);
}

fn decode_acc_body(bytes: &[u8], off: &mut usize) -> Option<ClusterAcc> {
    let k = read_u64(bytes, off)?;
    let n = read_u32(bytes, off)? as usize;
    let mut sum_m = Vec::with_capacity(n);
    for _ in 0..n {
        sum_m.push(Fx::from_raw(Goldilocks::new(read_u64(bytes, off)?)));
    }
    let sn = read_u32(bytes, off)? as usize;
    let mut seen = BTreeSet::new();
    for _ in 0..sn {
        let mid = read32(bytes, off)?;
        let nonce = read_u64(bytes, off)?;
        seen.insert((mid, nonce));
    }
    let commitment = read32(bytes, off)?;
    Some(ClusterAcc {
        sum_m,
        k,
        seen,
        commitment,
    })
}

fn read32(bytes: &[u8], off: &mut usize) -> Option<[u8; 32]> {
    if *off + 32 > bytes.len() {
        return None;
    }
    let a: [u8; 32] = bytes[*off..*off + 32].try_into().ok()?;
    *off += 32;
    Some(a)
}

fn read_u64(bytes: &[u8], off: &mut usize) -> Option<u64> {
    if *off + 8 > bytes.len() {
        return None;
    }
    let v = u64::from_le_bytes(bytes[*off..*off + 8].try_into().ok()?);
    *off += 8;
    Some(v)
}

fn read_u32(bytes: &[u8], off: &mut usize) -> Option<u32> {
    if *off + 4 > bytes.len() {
        return None;
    }
    let v = u32::from_le_bytes(bytes[*off..*off + 4].try_into().ok()?);
    *off += 4;
    Some(v)
}

fn read_u128(bytes: &[u8], off: &mut usize) -> Option<u128> {
    if *off + 16 > bytes.len() {
        return None;
    }
    let v = u128::from_le_bytes(bytes[*off..*off + 16].try_into().ok()?);
    *off += 16;
    Some(v)
}

/// Build a claim announce message.
pub fn claim_announce(topic: Topic, claim: RewardClaim) -> SettleMsg {
    SettleMsg::ClaimAnnounce {
        topic,
        claim_id: claim.id,
        neuron: claim.neuron,
        claim,
    }
}

/// Topic id for an epoch's claims_root (same bytes as TopicId for iroh-gossip).
pub fn topic_from_claims_root(claims_root: &[u8; 32]) -> Topic {
    *claims_root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewards::claim_from_links;
    use crate::tickets::{easy_target, grind_settlement, self_fold};
    use crate::settlement::Contribution;
    use tru::{Context, FocusingParams, Link as TLink};

    fn h(b: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        x[0] = b;
        x
    }

    #[test]
    fn claim_roundtrip() {
        let claim = claim_from_links(
            h(0xA1),
            h(10),
            vec![TLink::stake(h(2), h(1), 8000)],
            1,
        );
        let msg = claim_announce(h(0xC1), claim.clone());
        let bytes = encode_settle_msg(&msg);
        let dec = decode_settle_msg(&bytes).unwrap();
        match dec {
            SettleMsg::ClaimAnnounce { claim: c, .. } => {
                assert_eq!(c.id, claim.id);
                assert_eq!(c.neuron, claim.neuron);
                assert_eq!(c.links.len(), 1);
                assert_eq!(c.links[0].amount, 8000);
            }
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn self_acc_roundtrip() {
        let base = vec![
            TLink::stake(h(1), h(2), 100),
            TLink::stake(h(2), h(3), 100),
            TLink::stake(h(3), h(1), 100),
        ];
        let contribs = vec![Contribution {
            neuron: h(10),
            links: vec![TLink::stake(h(2), h(1), 8000)],
            surprise: Fx::ONE,
        }];
        let tickets = grind_settlement(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &h(0xBE),
            &h(0xC1),
            &h(0x91),
            0,
            16,
            2,
            easy_target(),
        );
        let acc = self_fold(contribs.len(), &tickets);
        let msg = SettleMsg::SelfAcc {
            topic: h(0xC1),
            miner: h(0x91),
            acc: acc.clone(),
        };
        let bytes = encode_settle_msg(&msg);
        let frame = encode_frame(&bytes);
        let (body, rest) = split_frame(&frame).unwrap();
        assert!(rest.is_empty());
        let dec = decode_settle_msg(&body).unwrap();
        match dec {
            SettleMsg::SelfAcc { acc: a, .. } => {
                assert_eq!(a.k, acc.k);
                assert_eq!(a.commitment, acc.commitment);
            }
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn receipt_roundtrip() {
        let msg = SettleMsg::ReceiptHash {
            topic: h(1),
            receipt_hash: h(2),
            epoch: 42,
        };
        let dec = decode_settle_msg(&encode_settle_msg(&msg)).unwrap();
        match dec {
            SettleMsg::ReceiptHash { epoch, .. } => assert_eq!(epoch, 42),
            _ => panic!("wrong"),
        }
    }

}
