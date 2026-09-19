//! Property #3 of launch.md's registry, row 9: nodes converge from different
//! starting states. Two nodes that receive the same cyberlinks in different
//! arrival orders build [`tru::FocusingGraph`] with different internal node
//! indices (assigned by first appearance — see `FocusingGraph::build`), yet
//! must compute the identical φ* per particle and agree on every fork-choice.
//! This is the single-process lemma behind [[foculus]]'s claim that gossip
//! delivery order never matters ("verified eventual consistency",
//! docs/explanation/interplanetary.md); the network gate on three real nodes
//! remains the row's full evidence.

use std::collections::HashMap;

use foculus::chain::{CyberlinkRecord, SELF_NETWORK, Signal};
use foculus::fork::{ForkChoice, LinksView};
use foculus::focus::Focus;
use tru::{Context, FocusingGraph, FocusingParams, Fx, Link, compute_focusing};

fn p(b: u8) -> [u8; 32] {
    [b; 32]
}

fn link(neuron: u8, from: u8, to: u8, amount: u64) -> CyberlinkRecord {
    CyberlinkRecord {
        neuron: p(neuron),
        from: p(from),
        to: p(to),
        token: p(0),
        amount,
        valence: 1,
        height: 0,
    }
}

fn sig_to(neuron: u8, step: u64, to: u8) -> Signal {
    Signal {
        neuron: p(neuron),
        network: SELF_NETWORK,
        links: vec![link(neuron, 9, to, 1)],
        delta_pi: vec![],
        box_moves: vec![],
        prev: p(0),
        step,
        height: 0,
        proof: None,
    }
}

/// A deterministic reordering (a fixed non-identity permutation) so the test
/// is reproducible across runs and platforms — not a random shuffle.
fn reordered<T: Clone>(items: &[T]) -> Vec<T> {
    let n = items.len();
    (0..n).map(|i| items[(i * 7 + 3) % n].clone()).collect()
}

/// φ* per particle, keyed by particle id — order-independent for comparison.
fn focus_by_particle(links: Vec<Link>) -> HashMap<[u8; 32], Fx> {
    let graph = FocusingGraph::build(links, &Context::none());
    let result = compute_focusing(&graph, &FocusingParams::default());
    graph
        .node_ids()
        .iter()
        .copied()
        .zip(result.focus)
        .collect()
}

#[test]
fn focus_distribution_identical_regardless_of_link_arrival_order() {
    let context = vec![
        link(2, 2, 1, 1000),
        link(3, 3, 1, 900),
        link(4, 4, 5, 700),
        link(5, 5, 6, 400),
        link(6, 6, 1, 250),
        link(7, 7, 5, 600),
        link(1, 9, 1, 1),
        link(1, 9, 7, 1),
    ];
    let as_links: Vec<Link> = context
        .iter()
        .map(|l| Link::stake(l.from, l.to, l.amount as u128))
        .collect();

    let arrival_order_a = as_links.clone();
    let arrival_order_b = reordered(&as_links);
    let sig = |links: &[Link]| -> Vec<([u8; 32], [u8; 32], u128)> {
        links.iter().map(|l| (l.from, l.to, l.amount)).collect()
    };
    assert_ne!(
        sig(&arrival_order_a),
        sig(&arrival_order_b),
        "the reordering must actually differ, or this test proves nothing"
    );

    let phi_a = focus_by_particle(arrival_order_a);
    let phi_b = focus_by_particle(arrival_order_b);

    assert_eq!(
        phi_a.len(),
        phi_b.len(),
        "both arrival orders must discover the same set of particles"
    );
    for (particle, value_a) in &phi_a {
        let value_b = phi_b
            .get(particle)
            .expect("every particle from order A must also appear under order B");
        assert_eq!(
            value_a, value_b,
            "φ* for particle {particle:?} must be bit-identical regardless of arrival order"
        );
    }
}

#[test]
fn fork_choice_agrees_regardless_of_context_arrival_order() {
    let context = vec![
        link(2, 2, 1, 1000),
        link(3, 3, 1, 1000),
        link(4, 4, 1, 1000),
        link(5, 5, 1, 1000),
        link(1, 9, 1, 1),
        link(1, 9, 7, 1),
    ];
    let view_a = LinksView(context.clone());
    let view_b = LinksView(reordered(&context));

    let a = sig_to(1, 0, 1);
    let b = sig_to(1, 0, 7);
    let members = vec![a.clone(), b.clone()];

    let winner_a = Focus::new().resolve(&members, &view_a).unwrap();
    let winner_b = Focus::new().resolve(&members, &view_b).unwrap();

    assert_eq!(
        members[winner_a].content_id(),
        members[winner_b].content_id(),
        "two nodes starting from differently-ordered link deliveries must \
         converge on the same fork-choice winner"
    );
    assert_eq!(
        members[winner_a].content_id(),
        a.content_id(),
        "the hub-directed signal should still win under either arrival order"
    );
}
