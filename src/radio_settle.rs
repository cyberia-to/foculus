// ---
// tags: foculus, rust, radio, settle, iroh
// crystal-type: source
// crystal-domain: cyber
// ---
//! Radio settle transport — real iroh QUIC push of settle-gossip frames.
//!
//! ALPN `foculus/settle/1`: each connection carries length-prefixed
//! [`crate::wire`] frames. Peers are known endpoint ids (tickets / mDNS).
//! Topic filter = claims_root; epidemic fanout to all connected peers that
//! share a subscription (or all peers if unfiltered).
//!
//! This is the production path for multi-miner SelfAcc / claim gossip.
//! Complements the in-process [`crate::gossip::SettleMesh`] used in unit tests.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::net::SocketAddrV4;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use iroh::address_lookup::MdnsAddressLookup;
use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, SecretKey};
use tokio::sync::RwLock;

use crate::gossip::{SettleMsg, Topic};
use crate::tickets::ClusterAcc;
use crate::wire::{decode_settle_msg, encode_frame, encode_settle_msg, split_frame};

/// ALPN for settle gossip streams.
pub const SETTLE_ALPN: &[u8] = b"foculus/settle/1";

/// Shared radio state: inbox, seen set, topics, peer book.
#[derive(Default)]
struct RadioState {
    inbox: VecDeque<SettleMsg>,
    seen: BTreeSet<[u8; 32]>,
    /// Topics we accept (empty = accept all).
    subs: BTreeSet<Topic>,
    /// Known peer addresses for outbound fanout (keyed by endpoint id).
    peers: BTreeMap<EndpointId, EndpointAddr>,
}

/// Protocol handler for inbound settle streams.
#[derive(Clone)]
struct SettleProtocol {
    state: Arc<RwLock<RadioState>>,
}

impl std::fmt::Debug for SettleProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SettleProtocol")
    }
}

impl ProtocolHandler for SettleProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let _ = handle_inbound(conn, self.state.clone()).await;
        Ok(())
    }
}

async fn handle_inbound(conn: Connection, state: Arc<RwLock<RadioState>>) -> Result<()> {
    let mut stream = conn.accept_uni().await?;
    let mut buf = Vec::new();
    loop {
        let mut chunk = [0u8; 8192];
        let n = match stream.read(&mut chunk).await? {
            None | Some(0) => break,
            Some(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        while let Some((frame, rest)) = split_frame(&buf) {
            buf = rest.to_vec();
            if let Some(msg) = decode_settle_msg(&frame) {
                ingest(&state, msg).await;
            }
        }
    }
    Ok(())
}

async fn ingest(state: &Arc<RwLock<RadioState>>, msg: SettleMsg) {
    let cid = msg.content_id();
    let topic = msg.topic();
    let mut st = state.write().await;
    if !st.subs.is_empty() && !st.subs.contains(&topic) {
        return;
    }
    if st.seen.insert(cid) {
        st.inbox.push_back(msg);
    }
}

/// Radio settle node — bind iroh, accept settle ALPN, fanout to peers.
pub struct SettleRadio {
    endpoint: Endpoint,
    state: Arc<RwLock<RadioState>>,
    _router: Router,
    /// Optional memory lookup for tests (holds peer addrs).
    memory_lookup: Option<MemoryLookup>,
}

impl SettleRadio {
    /// Start on a fixed port with mDNS discovery (production-like).
    pub async fn start(data_dir: &Path, port: u16) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let key = load_or_create_key(&data_dir.join("settle_secret.key"))?;
        let bind = SocketAddrV4::new(std::net::Ipv4Addr::UNSPECIFIED, port);
        let endpoint = Endpoint::builder()
            .relay_mode(RelayMode::Disabled)
            .secret_key(key)
            .address_lookup(MdnsAddressLookup::builder())
            .bind_addr(bind)
            .context("bind addr")?
            .bind()
            .await
            .context("bind endpoint")?;
        Self::from_endpoint(endpoint, None).await
    }

    /// Start with a shared MemoryLookup (multi-endpoint tests, no mDNS).
    pub async fn start_memory(lookup: MemoryLookup, secret: SecretKey) -> Result<Self> {
        let endpoint = Endpoint::builder()
            .relay_mode(RelayMode::Disabled)
            .secret_key(secret)
            .address_lookup(lookup.clone())
            .bind()
            .await
            .context("bind memory endpoint")?;
        // Publish our addr into the shared lookup so peers can dial us.
        lookup.add_endpoint_info(endpoint.addr());
        Self::from_endpoint(endpoint, Some(lookup)).await
    }

    async fn from_endpoint(endpoint: Endpoint, memory_lookup: Option<MemoryLookup>) -> Result<Self> {
        let state = Arc::new(RwLock::new(RadioState::default()));
        let proto = SettleProtocol {
            state: state.clone(),
        };
        let router = Router::builder(endpoint.clone())
            .accept(SETTLE_ALPN, proto)
            .spawn();
        Ok(Self {
            endpoint,
            state,
            _router: router,
            memory_lookup,
        })
    }

    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    pub fn endpoint_addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    /// Add peer with full address (registers into memory lookup if present).
    pub async fn add_peer_addr(&self, addr: EndpointAddr) {
        if let Some(lookup) = &self.memory_lookup {
            lookup.add_endpoint_info(addr.clone());
        }
        self.state.write().await.peers.insert(addr.id, addr);
    }

    /// Add peer by endpoint id only (relies on mDNS / prior lookup).
    pub async fn add_peer_id(&self, id: EndpointId) {
        self.state
            .write()
            .await
            .peers
            .entry(id)
            .or_insert_with(|| EndpointAddr::new(id));
    }

    /// Subscribe to a claims_root topic (empty subs = accept all).
    pub async fn subscribe(&self, topic: Topic) {
        self.state.write().await.subs.insert(topic);
    }

    /// Publish a settle message: local ingest + fanout to all known peers.
    pub async fn publish(&self, msg: SettleMsg) -> Result<usize> {
        let body = encode_settle_msg(&msg);
        let frame = encode_frame(&body);
        // Local
        ingest(&self.state, msg).await;

        let peers: Vec<EndpointAddr> = self.state.read().await.peers.values().cloned().collect();
        let me = self.endpoint.id();
        let mut sent = 0usize;
        for peer in peers {
            if peer.id == me {
                continue;
            }
            match self.push_to_peer(peer, &frame).await {
                Ok(()) => sent += 1,
                Err(_e) => {
                    // peer offline / dial failed — skip
                }
            }
        }
        Ok(sent)
    }

    async fn push_to_peer(&self, peer: EndpointAddr, frame: &[u8]) -> Result<()> {
        let conn = self
            .endpoint
            .connect(peer.clone(), SETTLE_ALPN)
            .await
            .with_context(|| format!("connect {}", peer.id))?;
        let mut send = conn.open_uni().await.context("open uni")?;
        send.write_all(frame).await.context("write")?;
        send.finish().context("finish")?;
        // allow peer to accept and read before we drop
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok(())
    }

    /// Drain local inbox.
    pub async fn drain(&self) -> Vec<SettleMsg> {
        let mut st = self.state.write().await;
        st.inbox.drain(..).collect()
    }

    /// Collect SelfAcc messages for a topic (drain-filter).
    pub async fn collect_self_accs(&self, topic: &Topic) -> Vec<ClusterAcc> {
        let msgs = self.drain().await;
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
        if !rest.is_empty() {
            let mut st = self.state.write().await;
            for m in rest {
                st.inbox.push_back(m);
            }
        }
        accs
    }

    /// Collect claim announces for a topic.
    pub async fn collect_claims(&self, topic: &Topic) -> Vec<crate::rewards::RewardClaim> {
        let msgs = self.drain().await;
        let mut claims = Vec::new();
        let mut rest = Vec::new();
        for m in msgs {
            match m {
                SettleMsg::ClaimAnnounce {
                    topic: t, claim, ..
                } if t == *topic => claims.push(claim),
                other => rest.push(other),
            }
        }
        if !rest.is_empty() {
            let mut st = self.state.write().await;
            for m in rest {
                st.inbox.push_back(m);
            }
        }
        claims
    }

    /// Wait up to `timeout` polling for at least `want` SelfAccs on topic.
    pub async fn wait_self_accs(
        &self,
        topic: &Topic,
        want: usize,
        timeout: Duration,
    ) -> Vec<ClusterAcc> {
        let start = std::time::Instant::now();
        let mut accs = Vec::new();
        while start.elapsed() < timeout {
            let batch = self.collect_self_accs(topic).await;
            accs.extend(batch);
            // dedupe by commitment
            let mut seen = BTreeSet::new();
            accs.retain(|a| seen.insert(a.commitment));
            if accs.len() >= want {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        accs
    }

    /// Endpoint id as hex string (for CLI tickets).
    pub fn id_string(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// JSON endpoint addr ticket.
    pub fn addr_json(&self) -> String {
        serde_json::to_string(&self.endpoint.addr()).unwrap_or_default()
    }

    pub async fn shutdown(self) -> Result<()> {
        self._router.shutdown().await.ok();
        Ok(())
    }
}

fn load_or_create_key(path: &Path) -> Result<SecretKey> {
    if path.exists() {
        let bytes = std::fs::read(path)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("bad settle secret key"))?;
        Ok(SecretKey::from(arr))
    } else {
        let key = SecretKey::generate(&mut rand::rng());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, key.to_bytes())?;
        Ok(key)
    }
}

/// Multi-miner settle over radio: each miner publishes SelfAcc; coordinator
/// collects peer accs and runs `settle_with_peer_accs`.
pub struct RadioSettleSession {
    pub radio: Arc<SettleRadio>,
    pub topic: Topic,
    pub miner: [u8; 32],
}

impl RadioSettleSession {
    pub fn new(radio: Arc<SettleRadio>, topic: Topic, miner: [u8; 32]) -> Self {
        Self {
            radio,
            topic,
            miner,
        }
    }

    pub async fn join_topic(&self) {
        self.radio.subscribe(self.topic).await;
    }

    pub async fn announce_claim(&self, claim: crate::rewards::RewardClaim) -> Result<usize> {
        self.radio
            .publish(crate::wire::claim_announce(self.topic, claim))
            .await
    }

    pub async fn publish_self_acc(&self, acc: ClusterAcc) -> Result<usize> {
        self.radio
            .publish(SettleMsg::SelfAcc {
                topic: self.topic,
                miner: self.miner,
                acc,
            })
            .await
    }

    pub async fn announce_receipt(&self, receipt_hash: [u8; 32], epoch: u64) -> Result<usize> {
        self.radio
            .publish(SettleMsg::ReceiptHash {
                topic: self.topic,
                receipt_hash,
                epoch,
            })
            .await
    }

    pub async fn peer_accs(&self, want: usize, timeout: Duration) -> Vec<ClusterAcc> {
        self.radio
            .wait_self_accs(&self.topic, want, timeout)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beacon::{claims_root, open_beacon, GENESIS_PREV, TEST_OUTER_T};
    use crate::rewards::{
        claim_from_links, contributions_with_rho, settle_with_peer_accs, verify_receipt,
        TicketPolicy,
    };
    use crate::tickets::{grind_settlement, self_fold, easy_target};
    use tru::{Context, FocusingParams, Link};

    fn h(b: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        x[0] = b;
        x
    }

    #[tokio::test]
    async fn two_endpoints_exchange_self_acc() {
        let lookup = MemoryLookup::new();
        let k1 = SecretKey::generate(&mut rand::rng());
        let k2 = SecretKey::generate(&mut rand::rng());
        let a = SettleRadio::start_memory(lookup.clone(), k1)
            .await
            .expect("a");
        let b = SettleRadio::start_memory(lookup.clone(), k2)
            .await
            .expect("b");

        // Cross-register peers
        a.add_peer_addr(b.endpoint_addr()).await;
        b.add_peer_addr(a.endpoint_addr()).await;

        let topic = h(0xC1);
        a.subscribe(topic).await;
        b.subscribe(topic).await;

        // Build a real self-acc
        let base = vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ];
        let contribs = vec![crate::settlement::Contribution {
            neuron: h(10),
            links: vec![Link::stake(h(2), h(1), 8000)],
            surprise: tru::Fx::ONE,
        }];
        let tickets = grind_settlement(
            &base,
            &contribs,
            &Context::none(),
            &FocusingParams::default(),
            &h(0xBE),
            &topic,
            &h(0x91),
            0,
            16,
            2,
            easy_target(),
        );
        let acc = self_fold(contribs.len(), &tickets);

        let sent = a
            .publish(SettleMsg::SelfAcc {
                topic,
                miner: h(0x91),
                acc: acc.clone(),
            })
            .await
            .expect("publish");
        assert!(sent >= 1, "should push to peer b");

        // Wait for delivery
        let mut got = Vec::new();
        for _ in 0..40 {
            got = b.collect_self_accs(&topic).await;
            if !got.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].commitment, acc.commitment);
        assert_eq!(got[0].k, acc.k);

        a.shutdown().await.ok();
        b.shutdown().await.ok();
    }

    #[tokio::test]
    async fn radio_multi_miner_settle_e2e() {
        let lookup = MemoryLookup::new();
        let miner_a = SettleRadio::start_memory(
            lookup.clone(),
            SecretKey::generate(&mut rand::rng()),
        )
        .await
        .unwrap();
        let miner_b = SettleRadio::start_memory(
            lookup.clone(),
            SecretKey::generate(&mut rand::rng()),
        )
        .await
        .unwrap();
        let settler = SettleRadio::start_memory(
            lookup.clone(),
            SecretKey::generate(&mut rand::rng()),
        )
        .await
        .unwrap();

        // Full mesh peers
        for (x, y) in [
            (&miner_a, &miner_b),
            (&miner_a, &settler),
            (&miner_b, &settler),
            (&miner_b, &miner_a),
            (&settler, &miner_a),
            (&settler, &miner_b),
        ] {
            x.add_peer_addr(y.endpoint_addr()).await;
        }

        let claims = vec![
            claim_from_links(h(0xA1), h(10), vec![Link::stake(h(2), h(1), 8000)], 1),
            claim_from_links(h(0xB2), h(11), vec![Link::stake(h(3), h(1), 6000)], 1),
        ];
        let ids: Vec<_> = claims.iter().map(|c| c.id).collect();
        let topic = claims_root(&ids);
        miner_a.subscribe(topic).await;
        miner_b.subscribe(topic).await;
        settler.subscribe(topic).await;

        let base = vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ];
        let art = open_beacon(1, &GENESIS_PREV, &topic, &[], TEST_OUTER_T);
        let b_e = art.beacon;
        let contribs = contributions_with_rho(&claims);

        // Each miner grinds and publishes SelfAcc over radio
        for (radio, miner_id, start) in [
            (&miner_a, h(0xA), 0u64),
            (&miner_b, h(0xB), 40u64),
        ] {
            let t = grind_settlement(
                &base,
                &contribs,
                &Context::none(),
                &FocusingParams::default(),
                &b_e,
                &topic,
                &miner_id,
                start,
                32,
                2,
                easy_target(),
            );
            let acc = self_fold(contribs.len(), &t);
            radio
                .publish(SettleMsg::SelfAcc {
                    topic,
                    miner: miner_id,
                    acc,
                })
                .await
                .unwrap();
        }

        // Settler collects peer accs over radio
        let peer_accs = settler
            .wait_self_accs(&topic, 2, Duration::from_secs(5))
            .await;
        assert!(
            peer_accs.len() >= 2,
            "settler should receive 2 SelfAccs, got {}",
            peer_accs.len()
        );

        let rec = settle_with_peer_accs(
            1,
            &GENESIS_PREV,
            &base,
            &claims,
            &Context::none(),
            &FocusingParams::default(),
            1000,
            &TicketPolicy {
                want: 2,
                max_attempts: 32,
                miner: h(0xCC),
                start_nonce: 200,
                ..TicketPolicy::default()
            },
            &peer_accs,
        )
        .unwrap();
        assert!(verify_receipt(&rec));
        let paid: u64 = rec.shares.iter().map(|s| s.amount).sum();
        assert_eq!(paid, 1000);

        miner_a.shutdown().await.ok();
        miner_b.shutdown().await.ok();
        settler.shutdown().await.ok();
    }
}
