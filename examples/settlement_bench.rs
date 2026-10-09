//! Sizes and times of foculus's sound settlement proofs.
//!
//! `cargo run --release --example settlement_bench`

use std::time::{Duration, Instant};

use foculus::cluster::{ClusterProof, verify_cluster};
use foculus::proof::{Proof, Relation};
use foculus::settlement::Contribution;
use foculus::ticket_proof::{TicketInputs, TicketRelation, prove_ticket, verify_ticket};
use foculus::tickets::{easy_target, grind_settlement};
use foculus::{CyberlinkRecord, Signal, TipProver, prove_pay, verify_pay};
use tru::{Context, FocusingParams, Fx, Link};

fn h(i: u64) -> [u8; 32] {
    let mut x = [0u8; 32];
    x[..8].copy_from_slice(&i.to_le_bytes());
    x[31] = 0xAA;
    x
}

struct Scale {
    base: Vec<Link>,
    contribs: Vec<Contribution>,
    ctx: Context,
    params: FocusingParams,
}

/// `particles` nodes on a ring with chords; `n` contributors, one link each.
fn scale(particles: u64, n: u64) -> Scale {
    let mut base = Vec::new();
    for i in 0..particles {
        base.push(Link::stake(h(i), h((i + 1) % particles), 100));
        base.push(Link::stake(h(i), h((i * 7 + 3) % particles), 40));
    }
    let contribs = (0..n)
        .map(|k| Contribution {
            neuron: h(1_000_000 + k),
            links: vec![Link::stake(h((k * 13 + 1) % particles), h((k * 5) % particles), 5000 + 100 * k as u128)],
            surprise: Fx::ONE,
        })
        .collect();
    Scale { base, contribs, ctx: Context::none(), params: FocusingParams::default() }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn main() {
    let (beacon, cluster) = (h(0xBE), h(0xC1));
    println!("## tickets: per-ticket proof (n contributors, base links)");
    println!("n | base links | free values | wire bytes | prove ms | verify ms | of which v3 rows ms");
    for (particles, n) in [(3, 1), (3, 3), (64, 8), (256, 16), (256, 64)] {
        let s = scale(particles, n);
        let inputs = TicketInputs { base: &s.base, contribs: &s.contribs, ctx: &s.ctx, params: &s.params };
        let t = grind_settlement(&s.base, &s.contribs, &s.ctx, &s.params, &beacon, &cluster, &h(7), 0, 4, 1, easy_target()).remove(0);
        let t0 = Instant::now();
        let p = prove_ticket(inputs, &beacon, &cluster, easy_target(), &t).unwrap();
        let prove = t0.elapsed();
        let t0 = Instant::now();
        verify_ticket(inputs, &beacon, &cluster, easy_target(), &p).unwrap();
        let verify = t0.elapsed();
        let rel = TicketRelation::new(inputs, &beacon, &cluster, &p.ticket);
        let Proof::Public(body) = &p.proof;
        let t0 = Instant::now();
        zheng::execution::certificate::verify(&rel.instance(), &body.certificate, &rel.public()).unwrap();
        let rows = t0.elapsed();
        println!(
            "{n} | {} | {} | {} | {:.3} | {:.3} | {:.4}",
            s.base.len(),
            body.certificate.free.len(),
            p.proof.wire_size(),
            ms(prove),
            ms(verify),
            ms(rows)
        );
    }

    println!("\n## cluster settlement verify (strict, every ticket)");
    println!("tickets | n | base links | verify ms | per ticket ms | proof bytes");
    for (particles, n) in [(3, 3), (64, 8)] {
        let s = scale(particles, n);
        let inputs = TicketInputs { base: &s.base, contribs: &s.contribs, ctx: &s.ctx, params: &s.params };
        let all = grind_settlement(&s.base, &s.contribs, &s.ctx, &s.params, &beacon, &cluster, &h(7), 0, 512, 512, easy_target());
        let proven: Vec<_> = all
            .iter()
            .map(|t| prove_ticket(inputs, &beacon, &cluster, easy_target(), t).unwrap())
            .collect();
        for k in [8usize, 64, 512] {
            let proof = ClusterProof { target: easy_target(), tickets: proven[..k].to_vec() };
            let bytes: usize = proof.tickets.iter().map(|t| t.proof.wire_size()).sum();
            let t0 = Instant::now();
            let acc = verify_cluster(inputs, &beacon, &cluster, &proof).unwrap();
            let d = t0.elapsed();
            assert_eq!(acc.k as usize, k);
            println!("{k} | {n} | {} | {:.2} | {:.4} | {bytes}", s.base.len(), ms(d), ms(d) / k as f64);
        }
    }

    println!("\n## pay proofs");
    println!("legs | free values | wire bytes | prove ms | verify ms");
    for legs in [1usize, 2, 16, 64] {
        let mut sig = Signal {
            neuron: h(1), network: [0; 32], links: vec![], delta_pi: vec![], box_moves: vec![],
            prev: [0; 32], step: 1, height: 0, proof: None,
        };
        sig.links = (0..legs)
            .map(|i| CyberlinkRecord { neuron: h(1), from: h(1), to: h(2 + i as u64), token: h(9), amount: 1_000_000 + i as u64, valence: 1, height: 0 })
            .collect();
        let t0 = Instant::now();
        let p = prove_pay(&sig).unwrap();
        let prove = t0.elapsed();
        let t0 = Instant::now();
        assert!(verify_pay(&p, &sig));
        let verify = t0.elapsed();
        let Proof::Public(body) = &p;
        println!("{legs} | {} | {} | {:.3} | {:.3}", body.certificate.free.len(), p.wire_size(), ms(prove), ms(verify));
    }

    println!("\n## tip chain (light join = recompute every step)");
    println!("steps | join ms | per step us");
    for steps in [1_000u64, 100_000] {
        let mut prover = TipProver::new();
        for i in 0..steps {
            prover.push_height(i, h(i)).unwrap();
        }
        let t0 = Instant::now();
        let tip = foculus::Tip::join_chain(prover.chain(), &prover.head());
        let d = t0.elapsed();
        assert!(tip.grade4());
        println!("{steps} | {:.2} | {:.3}", ms(d), ms(d) * 1e3 / steps as f64);
    }
}
