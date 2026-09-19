// ---
// tags: foculus, rust, finality, oikos
// crystal-type: source
// crystal-domain: cyber
// ---
//! Exercises specs/book-finality.md — oikos foundation 2 (domain finality per
//! book) on two independent books: book A settles a particle locally and
//! certifies it; book B's cross-book condition on that particle only resolves
//! once it holds and verifies A's evidence against A's own tip, and stays
//! unresolved while A has not certified.

use bbg::Checkpoint;
use foculus::{Domain, FinalityEvidence, Tip};
use tru::Fx;

fn fx(n: i64, d: i64) -> Fx {
    Fx::from_int(n).div(Fx::from_int(d))
}

fn spike_domain(hot: Fx) -> Domain {
    // one dominant particle over a flat low background clears μ + κ'σ
    Domain::from_focus(
        vec![[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32]],
        vec![hot, fx(5, 100), fx(5, 100), fx(5, 100)],
    )
}

/// A pending cross-book condition: book B waits for book A to certify
/// `signal_id`, and accepts only evidence that verifies against A's own tip.
fn resolve_cross_book_condition(evidence: Option<&FinalityEvidence>, source_tip: &Tip) -> bool {
    match evidence {
        Some(ev) => ev.verify(source_tip),
        None => false,
    }
}

#[test]
fn book_a_settles_locally_independent_of_book_b() {
    // Two books, two independent state spaces — different heights, different roots.
    let tip_a = Tip::from_local(&Checkpoint { root: [0xA0; 32], acc: None, height: 41 });
    let tip_b = Tip::from_local(&Checkpoint { root: [0xB0; 32], acc: None, height: 7 });

    let ev_a = FinalityEvidence::issue_local([1u8; 32], &tip_a, &[]);
    assert!(ev_a.verify(&tip_a), "book A settles the signal in its own view");
    assert!(
        !ev_a.verify(&tip_b),
        "book A's local evidence does not carry over to book B's unrelated tip"
    );
}

#[test]
fn cross_book_condition_waits_while_source_has_not_certified() {
    let tip_a = Tip::from_local(&Checkpoint { root: [0xA1; 32], acc: None, height: 41 });
    let signal = [7u8; 32];

    // The particle sits below the finality threshold in A's domain: A has not
    // certified it yet, so there is no evidence for B to accept.
    let cold_domain = spike_domain(fx(5, 100));
    let no_evidence = FinalityEvidence::issue_from_domain(
        signal, &tip_a, &[], fx(5, 100), &cold_domain,
        Fx::ZERO, fx(85, 1000), fx(74, 100), fx(225, 100), fx(15, 10),
    );
    assert!(no_evidence.is_none(), "A has not finalized; nothing to certify yet");
    assert!(
        !resolve_cross_book_condition(no_evidence.as_ref(), &tip_a),
        "book B's condition stays pending with no certified evidence from A"
    );
}

#[test]
fn cross_book_condition_resolves_once_source_certifies() {
    let tip_a = Tip::from_local(&Checkpoint { root: [0xA2; 32], acc: None, height: 41 });
    let signal = [7u8; 32];

    // The particle clears the adaptive threshold and the certification gate
    // in A's domain: A finalizes it locally and can hand out certified evidence.
    let hot_domain = spike_domain(fx(85, 100));
    let evidence = FinalityEvidence::issue_from_domain(
        signal, &tip_a, &[], fx(85, 100), &hot_domain,
        fx(3, 1000), fx(85, 1000), fx(74, 100), fx(225, 100), fx(15, 10),
    );
    assert!(evidence.is_some(), "A finalized the particle in its own domain");

    // Book B receives the evidence out of band and checks it against A's tip —
    // it never re-runs A's tri-kernel or consensus.
    assert!(
        resolve_cross_book_condition(evidence.as_ref(), &tip_a),
        "book B's condition resolves once A's certified evidence verifies"
    );

    // A stale or wrong source tip does not satisfy the condition — the
    // dependency is on A's actual state, not on B's say-so.
    let wrong_tip_a = Tip::from_local(&Checkpoint { root: [0xFF; 32], acc: None, height: 41 });
    assert!(!resolve_cross_book_condition(evidence.as_ref(), &wrong_tip_a));
}
