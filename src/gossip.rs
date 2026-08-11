// ---
// tags: foculus, rust, gossip, rewards, fold-mining
// crystal-type: source
// crystal-domain: cyber
// ---
//! Multi-node settle gossip — claims + self-accumulators (fold-mining mesh).
//!
//! Spec: `specs/gossip.md` (epidemic push, dedupe by content id, fanout ≥ 2)
//! applied to settlement messages. Transport-agnostic: an in-process
//! [`SettleMesh`] for tests / single-process multi-miner, plus encode/decode
//! Wire codec lives in [`crate::wire`]; live iroh transport in
//! [`crate::radio_settle`] (feature `net`).
//!
//! Messages:
//! - `Claim` — propose-window reward claim envelope
//! - `SelfAcc` — miner self-fold monoid for a cluster (fold-tree leaf)
//! - `Receipt` — optional settle receipt hash announcement

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cyber_hemera::hash as hemera_hash;
use tru::Fx;

use crate::rewards::RewardClaim;
use crate::tickets::ClusterAcc;

/// Topic = claims_root / cluster id.
pub type Topic = [u8; 32];

/// Wire / mesh message for settlement gossip.
#[derive(Clone)]
pub enum SettleMsg {
    /// Propose-window claim (neuron, id, links not re-serialized fully —
    /// peer re-builds from local view; we ship claim id + neuron + link count).
    ClaimAnnounce {
        topic: Topic,
        claim_id: [u8; 32],
        neuron: [u8; 32],
        /// Full claim for library mesh (local process). Wire path can strip.
        claim: RewardClaim,
    },
    /// Miner's self-folded accumulator for this cluster.
    SelfAcc {
        topic: Topic,
        miner: [u8; 32],
        acc: ClusterAcc,
    },
    /// Receipt hash announcement after local settle.
    ReceiptHash {
        topic: Topic,
        receipt_hash: [u8; 32],
        epoch: u64,
    },
}

impl SettleMsg {
    /// Content identity for dedupe.
    pub fn content_id(&self) -> [u8; 32] {
        match self {
            SettleMsg::ClaimAnnounce { claim_id, .. } => *claim_id,
            SettleMsg::SelfAcc { topic, miner, acc } => {
                let mut buf = Vec::with_capacity(96);
                buf.extend_from_slice(b"selfacc");
                buf.extend_from_slice(topic);
                buf.extend_from_slice(miner);
                buf.extend_from_slice(&acc.commitment);
                buf.extend_from_slice(&acc.k.to_le_bytes());
                hash32(&buf)
            }
            SettleMsg::ReceiptHash {
                topic,
                receipt_hash,
                epoch,
            } => {
                let mut buf = Vec::with_capacity(72);
                buf.extend_from_slice(b"rcpt");
                buf.extend_from_slice(topic);
                buf.extend_from_slice(receipt_hash);
                buf.extend_from_slice(&epoch.to_le_bytes());
                hash32(&buf)
            }
        }
    }

    pub fn topic(&self) -> Topic {
        match self {
            SettleMsg::ClaimAnnounce { topic, .. }
            | SettleMsg::SelfAcc { topic, .. }
            | SettleMsg::ReceiptHash { topic, .. } => *topic,
        }
    }
}

/// Encode a SelfAcc for transport (compact binary).
pub fn encode_self_acc(topic: &Topic, miner: &[u8; 32], acc: &ClusterAcc) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"FSC1"); // foculus self-acc v1
    out.extend_from_slice(topic);
    out.extend_from_slice(miner);
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
    out
}

/// Decode SelfAcc bytes.
pub fn decode_self_acc(bytes: &[u8]) -> Option<(Topic, [u8; 32], ClusterAcc)> {
    if bytes.len() < 4 + 32 + 32 + 8 + 4 {
        return None;
    }
    if &bytes[0..4] != b"FSC1" {
        return None;
    }
    let mut off = 4;
    let topic: Topic = bytes[off..off + 32].try_into().ok()?;
    off += 32;
    let miner: [u8; 32] = bytes[off..off + 32].try_into().ok()?;
    off += 32;
    let k = u64::from_le_bytes(bytes[off..off + 8].try_into().ok()?);
    off += 8;
    let n = u32::from_le_bytes(bytes[off..off + 4].try_into().ok()?) as usize;
    off += 4;
    if bytes.len() < off + n * 8 + 4 {
        return None;
    }
    let mut sum_m = Vec::with_capacity(n);
    for _ in 0..n {
        let raw = u64::from_le_bytes(bytes[off..off + 8].try_into().ok()?);
        off += 8;
        sum_m.push(Fx::from_raw(nebu::Goldilocks::new(raw)));
    }
    let sn = u32::from_le_bytes(bytes[off..off + 4].try_into().ok()?) as usize;
    off += 4;
    if bytes.len() < off + sn * 40 + 32 {
        return None;
    }
    let mut seen = BTreeSet::new();
    for _ in 0..sn {
        let mid: [u8; 32] = bytes[off..off + 32].try_into().ok()?;
        off += 32;
        let nonce = u64::from_le_bytes(bytes[off..off + 8].try_into().ok()?);
        off += 8;
        seen.insert((mid, nonce));
    }
    let commitment: [u8; 32] = bytes[off..off + 32].try_into().ok()?;
    Some((
        topic,
        miner,
        ClusterAcc {
            sum_m,
            k,
            seen,
            commitment,
        },
    ))
}

/// One mesh peer's inbox + seen set.
#[derive(Default)]
struct PeerState {
    inbox: VecDeque<SettleMsg>,
    seen: BTreeSet<[u8; 32]>,
    /// Subscribed topics (claims_root). Empty = all.
    subs: BTreeSet<Topic>,
}

/// In-process epidemic mesh for multi-miner settle tests and cell-local swarm.
pub struct SettleMesh {
    peers: BTreeMap<[u8; 32], PeerState>,
    fanout: usize,
}

impl SettleMesh {
    pub fn new(fanout: usize) -> Self {
        Self {
            peers: BTreeMap::new(),
            fanout: fanout.max(2),
        }
    }

    pub fn join(&mut self, peer: [u8; 32]) {
        self.peers.entry(peer).or_default();
    }

    pub fn subscribe(&mut self, peer: &[u8; 32], topic: Topic) {
        if let Some(p) = self.peers.get_mut(peer) {
            p.subs.insert(topic);
        }
    }

    /// Publish from `from` — deliver to self inbox + fanout to other peers.
    pub fn publish(&mut self, from: &[u8; 32], msg: SettleMsg) -> usize {
        let cid = msg.content_id();
        let topic = msg.topic();
        let mut delivered = 0usize;

        // Local deliver
        if let Some(p) = self.peers.get_mut(from) {
            if p.seen.insert(cid) {
                p.inbox.push_back(msg.clone());
                delivered += 1;
            }
        }

        // Fanout to others (deterministic order by peer id)
        let targets: Vec<[u8; 32]> = self
            .peers
            .keys()
            .filter(|id| *id != from)
            .filter(|id| {
                let p = &self.peers[*id];
                p.subs.is_empty() || p.subs.contains(&topic)
            })
            .copied()
            .take(self.fanout)
            .collect();

        for t in targets {
            if let Some(p) = self.peers.get_mut(&t) {
                if p.seen.insert(cid) {
                    p.inbox.push_back(msg.clone());
                    delivered += 1;
                }
            }
        }
        delivered
    }

    /// Drain inbox for peer.
    pub fn drain(&mut self, peer: &[u8; 32]) -> Vec<SettleMsg> {
        self.peers
            .get_mut(peer)
            .map(|p| p.inbox.drain(..).collect())
            .unwrap_or_default()
    }

    /// Collect all SelfAcc messages a peer has seen for a topic.
    pub fn peer_accs(&mut self, peer: &[u8; 32], topic: &Topic) -> Vec<ClusterAcc> {
        let msgs = self.drain(peer);
        // re-queue non-matching; extract accs
        let mut accs = Vec::new();
        let mut rest = Vec::new();
        for m in msgs {
            match &m {
                SettleMsg::SelfAcc { topic: t, acc, .. } if t == topic => {
                    accs.push(acc.clone());
                }
                _ => rest.push(m),
            }
        }
        if let Some(p) = self.peers.get_mut(peer) {
            for m in rest {
                p.inbox.push_back(m);
            }
        }
        accs
    }

    /// Collect claim announces for topic.
    pub fn peer_claims(&mut self, peer: &[u8; 32], topic: &Topic) -> Vec<RewardClaim> {
        let msgs = self.drain(peer);
        let mut claims = Vec::new();
        let mut rest = Vec::new();
        for m in msgs {
            match m {
                SettleMsg::ClaimAnnounce {
                    topic: t, claim, ..
                } if t == *topic => {
                    claims.push(claim);
                }
                other => rest.push(other),
            }
        }
        if let Some(p) = self.peers.get_mut(peer) {
            for m in rest {
                p.inbox.push_back(m);
            }
        }
        claims
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
}

fn hash32(buf: &[u8]) -> [u8; 32] {
    *hemera_hash(buf)
        .as_bytes()
        .first_chunk::<32>()
        .unwrap_or(&[0u8; 32])
}

// nebu for Goldilocks in decode
use nebu;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewards::claim_from_links;
    use crate::tickets::{easy_target, grind_settlement, self_fold};
    use crate::settlement::Contribution;
    use tru::{Context, FocusingParams, Link};

    fn h(b: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        x[0] = b;
        x
    }

    #[test]
    fn fanout_delivers_to_peers() {
        let mut mesh = SettleMesh::new(2);
        let a = h(1);
        let b = h(2);
        let c = h(3);
        mesh.join(a);
        mesh.join(b);
        mesh.join(c);
        let topic = h(0xC1);
        mesh.subscribe(&a, topic);
        mesh.subscribe(&b, topic);
        mesh.subscribe(&c, topic);

        let claim = claim_from_links(h(0xA1), a, vec![Link::stake(h(2), h(1), 100)], 1);
        let n = mesh.publish(
            &a,
            SettleMsg::ClaimAnnounce {
                topic,
                claim_id: claim.id,
                neuron: a,
                claim,
            },
        );
        assert!(n >= 2); // self + ≥1 peer
        let claims_b = mesh.peer_claims(&b, &topic);
        assert_eq!(claims_b.len(), 1);
    }

    #[test]
    fn self_acc_roundtrip_codec() {
        let base = vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ];
        let contribs = vec![Contribution {
            neuron: h(10),
            links: vec![Link::stake(h(2), h(1), 8000)],
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
        let bytes = encode_self_acc(&h(0xC1), &h(0x91), &acc);
        let (topic, miner, dec) = decode_self_acc(&bytes).unwrap();
        assert_eq!(topic, h(0xC1));
        assert_eq!(miner, h(0x91));
        assert_eq!(dec.k, acc.k);
        assert_eq!(dec.commitment, acc.commitment);
        assert_eq!(dec.seen, acc.seen);
    }

    #[test]
    fn multi_miner_accs_collect() {
        let mut mesh = SettleMesh::new(3);
        let m1 = h(0xA);
        let m2 = h(0xB);
        let settler = h(0xCC);
        mesh.join(m1);
        mesh.join(m2);
        mesh.join(settler);
        let topic = h(0xC1);
        for p in [&m1, &m2, &settler] {
            mesh.subscribe(p, topic);
        }

        let base = vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ];
        let contribs = vec![
            Contribution {
                neuron: h(10),
                links: vec![Link::stake(h(2), h(1), 8000)],
                surprise: Fx::ONE,
            },
            Contribution {
                neuron: h(11),
                links: vec![Link::stake(h(3), h(1), 6000)],
                surprise: Fx::ONE,
            },
        ];
        for (miner, start) in [(m1, 0u64), (m2, 100u64)] {
            let t = grind_settlement(
                &base,
                &contribs,
                &Context::none(),
                &FocusingParams::default(),
                &h(0xBE),
                &topic,
                &miner,
                start,
                16,
                2,
                easy_target(),
            );
            let acc = self_fold(contribs.len(), &t);
            mesh.publish(
                &miner,
                SettleMsg::SelfAcc {
                    topic,
                    miner,
                    acc,
                },
            );
        }
        let accs = mesh.peer_accs(&settler, &topic);
        assert_eq!(accs.len(), 2);
        let k: u64 = accs.iter().map(|a| a.k).sum();
        assert!(k >= 4);
    }

    #[test]
    fn dedupe_same_content() {
        let mut mesh = SettleMesh::new(2);
        let a = h(1);
        let b = h(2);
        mesh.join(a);
        mesh.join(b);
        let topic = h(9);
        let claim = claim_from_links(h(0xA1), a, vec![], 1);
        let msg = SettleMsg::ClaimAnnounce {
            topic,
            claim_id: claim.id,
            neuron: a,
            claim,
        };
        mesh.publish(&a, msg.clone());
        mesh.publish(&a, msg);
        // b should see at most one
        let c = mesh.peer_claims(&b, &topic);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn multi_miner_settle_with_gossiped_accs() {
        use crate::beacon::GENESIS_PREV;
        use crate::rewards::{claim_from_links, settle_with_peer_accs, verify_receipt, TicketPolicy};

        let mut mesh = SettleMesh::new(3);
        let m1 = h(0xA);
        let m2 = h(0xB);
        let settler = h(0xCC);
        mesh.join(m1);
        mesh.join(m2);
        mesh.join(settler);

        let base = vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ];
        let claims = vec![
            claim_from_links(h(0xA1), h(10), vec![Link::stake(h(2), h(1), 8000)], 1),
            claim_from_links(h(0xB2), h(11), vec![Link::stake(h(3), h(1), 6000)], 1),
        ];
        let ids: Vec<_> = claims.iter().map(|c| c.id).collect();
        let topic = crate::beacon::claims_root(&ids);
        for p in [&m1, &m2, &settler] {
            mesh.subscribe(p, topic);
        }

        // Each miner grinds + gossips self-acc (as if after beacon known).
        // Use settle_with_peer_accs path: first compute beacon via quiet settle grind helpers.
        let policy_template = |miner: [u8; 32], start: u64| TicketPolicy {
            want: 2,
            max_attempts: 32,
            start_nonce: start,
            miner,
            ..TicketPolicy::default()
        };

        // Publish self-accs by grinding against a provisional beacon (mesh demo).
        let art = crate::beacon::open_beacon(1, &GENESIS_PREV, &topic, &[], crate::beacon::TEST_OUTER_T);
        let b_e = art.beacon;
        let contribs = crate::rewards::contributions_with_rho(&claims);
        for (miner, start) in [(m1, 0u64), (m2, 50u64)] {
            let t = grind_settlement(
                &base,
                &contribs,
                &Context::none(),
                &FocusingParams::default(),
                &b_e,
                &topic,
                &miner,
                start,
                32,
                2,
                easy_target(),
            );
            let acc = self_fold(contribs.len(), &t);
            mesh.publish(
                &miner,
                SettleMsg::SelfAcc {
                    topic,
                    miner,
                    acc,
                },
            );
        }
        let peer_accs = mesh.peer_accs(&settler, &topic);
        assert_eq!(peer_accs.len(), 2);

        let rec = settle_with_peer_accs(
            1,
            &GENESIS_PREV,
            &base,
            &claims,
            &Context::none(),
            &FocusingParams::default(),
            1000,
            &policy_template(settler, 200),
            &peer_accs,
        )
        .unwrap();
        assert!(verify_receipt(&rec));
        assert!(rec.sample_count >= 2);
        let paid: u64 = rec.shares.iter().map(|s| s.amount).sum();
        assert_eq!(paid, 1000);
        let _ = policy_template;
    }
}
