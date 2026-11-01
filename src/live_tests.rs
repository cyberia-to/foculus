use super::*;

fn h(b: u8) -> [u8; 32] {
    let mut x = [0u8; 32];
    x[0] = b;
    x
}

fn base() -> Vec<Link> {
    vec![
        Link::stake(h(1), h(2), 100),
        Link::stake(h(2), h(3), 100),
        Link::stake(h(3), h(1), 100),
    ]
}

#[test]
fn full_node_link_settle_and_mature() {
    let mut full = LiveNode::new(NodeMode::Full, h(10));
    full.reward_base = base();
    full.budget = 500;
    full.settle_depth = 1;
    full.link(h(2), h(1), 8000, 1).unwrap();
    assert!(full.grade4());
    let cert = full.close_and_settle_epoch().unwrap();
    let rec = cert.settle.as_ref().expect("settle");
    assert!(verify_receipt(rec));
    assert_eq!(share_of(rec, &h(10)), 500);
    assert!(full.pending_rewards.iter().any(|(a, _, _)| *a == 500));
    full.bump_tip().unwrap(); // mature depth 1
    assert_eq!(full.matured_reward, 500);
    // cert clone stays verifiable — from the public claims, never without them
    assert!(verifies_from_signals(&full, &cert.clone()));
    assert!(!verify_epoch_cert(&cert, None));
}

fn verifies_from_signals(node: &LiveNode, cert: &EpochCertificate) -> bool {
    let claims: Vec<_> = node
        .signals
        .values()
        .map(|s| claim_from_links(s.id, s.neuron, s.links.clone(), s.valence))
        .collect();
    let (ctx, params) = (Context::none(), FocusingParams::default());
    let inputs = SettleVerifyInputs {
        base: &node.reward_base,
        claims: &claims,
        ctx: &ctx,
        params: &params,
        target: easy_target(),
    };
    verify_epoch_cert(cert, Some(&inputs))
}

#[test]
fn multi_node_swarm_leaderless_settle() {
    let mut alice = LiveNode::new(NodeMode::Full, h(10));
    let mut bob = LiveNode::new(NodeMode::Full, h(11));
    let mut carol = LiveNode::new(NodeMode::Light, h(12));
    alice.reward_base = base();
    bob.reward_base = base();
    alice.budget = 1000;
    bob.budget = 1000;
    alice.settle_depth = 0;
    bob.settle_depth = 0;
    carol.settle_depth = 0;

    alice.link(h(2), h(1), 8000, 1).unwrap();
    bob.link(h(3), h(1), 6000, 1).unwrap();

    // Cross-gossip signals
    for s in bob.signals.values().cloned().collect::<Vec<_>>() {
        alice.ingest_signal(s).ok();
    }
    for s in alice.signals.values().cloned().collect::<Vec<_>>() {
        bob.ingest_signal(s).ok();
    }

    // Shared claim set on alice's epoch runner
    alice.epoch = EpochRunner::genesis(1);
    alice.epoch.budget = 1000;
    for s in alice.signals.values() {
        alice
            .epoch
            .propose(claim_from_links(s.id, s.neuron, s.links.clone(), s.valence))
            .unwrap();
    }
    alice.freeze_and_beacon().unwrap();

    // Bob mirrors claim set + beacon (network agreement)
    bob.epoch = EpochRunner::genesis(1);
    bob.epoch.budget = 1000;
    for s in alice.signals.values() {
        bob.epoch
            .propose(claim_from_links(s.id, s.neuron, s.links.clone(), s.valence))
            .unwrap();
    }
    bob.epoch.freeze().unwrap();
    bob.epoch.beacon = alice.epoch.beacon.clone();
    bob.epoch.phase = EpochPhase::BeaconReady;
    bob.epoch.claims_root = alice.epoch.claims_root;

    let acc_b = bob.mine_self_acc().unwrap();
    let acc_a = alice.mine_self_acc().unwrap();
    alice.absorb_peer_acc(acc_b);
    alice.absorb_peer_acc(acc_a);

    let cert = alice.close_and_settle_epoch().unwrap();
    let rec = cert.settle.as_ref().unwrap();
    assert_eq!(rec.shares.iter().map(|s| s.amount).sum::<u64>(), 1000);
    assert!(rec.sample_count >= 2);

    // Light accepts cert (idempotent)
    let claims: Vec<_> = alice
        .signals
        .values()
        .map(|s| claim_from_links(s.id, s.neuron, s.links.clone(), s.valence))
        .collect();
    carol.reward_base = base();
    carol.accept_epoch_cert(cert.clone(), Some(&claims)).unwrap();
    carol.accept_epoch_cert(cert, Some(&claims)).unwrap(); // no double credit
    assert!(carol.tip().height > 0);
    assert!(carol.last_cert.is_some());
}

#[test]
fn partial_mode_same_as_full_for_local_settle() {
    let mut partial = LiveNode::new(NodeMode::Partial, h(10));
    partial.reward_base = base();
    partial.budget = 200;
    partial.settle_depth = 0;
    partial.link(h(2), h(1), 5000, 1).unwrap();
    let cert = partial.close_and_settle_epoch().unwrap();
    assert!(verifies_from_signals(&partial, &cert));
    assert_eq!(partial.matured_reward, 200);
}

#[test]
fn light_cannot_ingest() {
    let mut light = LiveNode::new(NodeMode::Light, h(1));
    assert_eq!(light.link(h(2), h(1), 1, 1), Err(LiveError::WrongMode));
}

#[test]
fn mine_requires_beacon() {
    let mut n = LiveNode::new(NodeMode::Full, h(10));
    n.reward_base = base();
    n.link(h(2), h(1), 100, 1).unwrap();
    assert!(matches!(n.mine_self_acc(), Err(LiveError::BeaconNotReady)));
}
