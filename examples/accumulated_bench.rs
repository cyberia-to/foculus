//! Cluster settlement: native per-ticket verification against one zheng
//! accumulation per cluster.
//!
//! `cargo run --release --features accumulated-settlement --example accumulated_bench`

use std::time::{Duration, Instant};

use foculus::cluster::{ClusterProof, verify_cluster};
use foculus::cluster_accumulated::{
    AccParams, check_values, prove_cluster_accumulated, verify_cluster_accumulated, verify_folded_part,
};
use foculus::settlement::Contribution;
use foculus::ticket_proof::{TicketInputs, prove_ticket};
use foculus::tickets::{easy_target, grind_settlement};
use tru::{Context, FocusingParams, Fx, Link};

fn h(i: u64) -> [u8; 32] {
    let mut x = [0u8; 32];
    x[..8].copy_from_slice(&i.to_le_bytes());
    x[31] = 0xAA;
    x
}

/// `particles` nodes on a ring with chords; `n` contributors, one link each
/// (the fixture of `settlement_bench`).
fn scale(particles: u64, n: u64) -> (Vec<Link>, Vec<Contribution>) {
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
    (base, contribs)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn main() {
    let (beacon, cluster) = (h(0xBE), h(0xC1));
    let (ctx, fparams) = (Context::none(), FocusingParams::default());
    let reps = 3;
    println!(
        "tickets | n | chunk | native prove ms | native verify ms | native B | acc prove ms | acc B (claims / steps / decider / values) | acc verify ms | of which folded ms | of which native values ms"
    );
    for (particles, n) in [(3u64, 3u64), (64, 8)] {
        let (base, contribs) = scale(particles, n);
        let inputs = TicketInputs { base: &base, contribs: &contribs, ctx: &ctx, params: &fparams };
        let all = grind_settlement(&base, &contribs, &ctx, &fparams, &beacon, &cluster, &h(7), 0, 512, 512, easy_target());
        for (k, chunk) in [(16usize, 16usize), (512, 64), (512, 16)] {
            if n == 8 && chunk == 16 && k == 512 {
                continue;
            }
            let tickets = &all[..k];
            let t0 = Instant::now();
            let proven: Vec<_> = tickets
                .iter()
                .map(|t| prove_ticket(inputs, &beacon, &cluster, easy_target(), t).unwrap())
                .collect();
            let nprove = ms(t0.elapsed());
            let cp = ClusterProof { target: easy_target(), tickets: proven };
            let nbytes: usize = cp.tickets.iter().map(|t| t.proof.wire_size()).sum();
            let nverify = median(
                (0..reps)
                    .map(|_| {
                        let t0 = Instant::now();
                        verify_cluster(inputs, &beacon, &cluster, &cp).unwrap();
                        ms(t0.elapsed())
                    })
                    .collect(),
            );

            let params = AccParams { chunk, ..AccParams::settlement() };
            let t0 = Instant::now();
            let p = prove_cluster_accumulated(inputs, &beacon, &cluster, easy_target(), &params, tickets).unwrap();
            let aprove = ms(t0.elapsed());
            let mut w = lens::rspcs::wire::Writer::default();
            for c in &p.folded.claims {
                c.write(&mut w);
            }
            let claims = w.buf.len();
            let steps: usize = p.folded.steps.iter().map(|s| s.to_bytes().len()).sum();
            let mut d = lens::rspcs::wire::Writer::default();
            p.folded.decider.write(&mut d, true);
            let values = 8 * p.values.iter().map(Vec::len).sum::<usize>();
            let mut total = Vec::new();
            let mut folded = Vec::new();
            let mut native = Vec::new();
            for _ in 0..reps {
                let t0 = Instant::now();
                let acc = verify_cluster_accumulated(inputs, &beacon, &cluster, &params, &p).unwrap();
                total.push(ms(t0.elapsed()));
                assert_eq!(acc.k as usize, k);
                let t0 = Instant::now();
                verify_folded_part(inputs, &beacon, &cluster, &params, &p).unwrap();
                folded.push(ms(t0.elapsed()));
                let t0 = Instant::now();
                check_values(inputs, &beacon, &cluster, &p).unwrap();
                native.push(ms(t0.elapsed()));
            }
            println!(
                "{k} | {n} | {chunk} | {nprove:.1} | {nverify:.1} | {nbytes} | {aprove:.1} | {} ({claims} / {steps} / {} / {values}) | {:.1} | {:.1} | {:.1}",
                p.proof_bytes(),
                d.buf.len(),
                median(total),
                median(folded),
                median(native),
            );
        }
    }
}
