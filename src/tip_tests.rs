use super::*;

fn prover(heights: &[(u64, u8)]) -> TipProver {
    let mut p = TipProver::new();
    for &(h, r) in heights {
        p.push_height(h, [r; 32]).unwrap();
    }
    p
}

#[test]
fn local_tip_is_grade4() {
    let ck = Checkpoint {
        root: [1u8; 32],
        acc: None,
        height: 7,
    };
    let tip = Tip::from_local(&ck);
    assert!(tip.grade4());
    assert_eq!(tip.trust, TipTrust::LocalApplied);
}

#[test]
fn checkpoint_join_is_untrusted() {
    let ck = prover(&[(0, 1), (1, 2)]).checkpoint();
    assert!(!Tip::join_checkpoint(&ck).grade4());
}

#[test]
fn chain_join_under_trusted_head() {
    let p = prover(&[(0, 1), (1, 2), (2, 3)]);
    let tip = Tip::join_chain(p.chain(), &p.head());
    assert_eq!(tip.trust, TipTrust::ChainVerified);
    assert_eq!((tip.height, tip.root), (2, [3u8; 32]));
    assert_eq!(p.seal_tip().unwrap().head, Some(p.head()));
}

/// The gap the legacy fold tip had (a checkpoint root the fold never saw was
/// accepted) is closed: any changed step changes the head.
#[test]
fn a_root_the_chain_never_saw_is_not_trusted() {
    let p = prover(&[(4, 5)]);
    let mut chain = p.chain().clone();
    chain.steps[0].root = [6u8; 32];
    let tip = Tip::join_chain(&chain, &p.head());
    assert_eq!(tip.trust, TipTrust::Untrusted);
    let honest_head = p.head();
    let other = prover(&[(4, 6)]);
    assert_eq!(Tip::join_chain(other.chain(), &honest_head).trust, TipTrust::Untrusted);
}

#[test]
fn heights_must_strictly_increase() {
    let mut p = prover(&[(3, 1)]);
    assert_eq!(p.push_height(3, [2; 32]), Err(TipError::HeightRegression));
    let mut chain = p.chain().clone();
    chain.steps.push(TipStep { height: 2, root: [2; 32], leaves: [0; 32] });
    assert_eq!(chain.verify(), None);
    assert_eq!(TipChain::default().verify(), None);
}

#[test]
fn light_advance_follows_trusted_heads_only() {
    let mut p = prover(&[(0, 9)]);
    let mut tip = p.seal_tip().unwrap();
    let step = TipStep { height: 1, root: [8; 32], leaves: [0; 32] };
    p.push_height(1, [8; 32]).unwrap();
    assert_eq!(tip.advance_chain(step, &[0; 32]), Err(TipError::UntrustedHead));
    tip.advance_chain(step, &p.head()).unwrap();
    assert_eq!((tip.height, tip.root), (1, [8u8; 32]));
    assert!(tip.grade4());
    assert_eq!(tip.advance_chain(step, &p.head()), Err(TipError::HeightRegression));
    let mut local = Tip::from_local(&p.checkpoint());
    assert_eq!(local.advance_chain(step, &p.head()), Err(TipError::NotChainMode));
}

#[test]
fn join_with_demo_chain_is_grade4() {
    let tip = join_with_demo_chain([9u8; 32], 3);
    assert!(tip.grade4());
    assert_eq!((tip.root, tip.height), ([9u8; 32], 3));
}
