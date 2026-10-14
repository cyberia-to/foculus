//! Which proof each foculus statement carries — native recomputation, zheng
//! public certificate v3, or zheng profile 1 (one relation, committed
//! witness, `device_params`, held key) — measured on one backend.
//!
//! ```text
//! ZHENG_BACKEND=cpu   cargo run --release --example device_profiles
//! ZHENG_BACKEND=metal cargo run --release --example device_profiles
//! ```
//!
//! `DP_REPS` (default 5) repetitions, median reported, times in ms.
//! Columns: `cold` = key derivation + prove (a device's first proof of the
//! shape), `warm` = prove with the held key, `verify` = the verifier as the
//! product calls it (relation derived from public data), `bytes` = on the
//! wire (profile + statement + body).
//!
//! Tip step and fold merge have no CCS in foculus: their statement is a
//! hemera hash over public data. Their profile-1 alternative is measured as
//! the nox program hashing the same number of field elements (13 for a tip
//! step: prev head 4, height 1, root 4, leaves 4; 12 for a fold merge),
//! proven with zheng's own `ProvingKey`.
//!
//! The ticket relation is not closed (its coalition values are checked
//! natively against `tru` — no CCS for the impulse exists), so profile 1
//! is not sound for it. Its row is the cost of the relation alone under a
//! bench-only adapter, to show what hiding the witness would buy: the
//! verifier would still recompute the impulse.

use std::time::Instant;

use foculus::pay_proof::PayRelation;
use foculus::proof::{self, Relation, RelationKey, Witnessed};
use foculus::settlement::Contribution;
use foculus::ticket_proof::{TicketInputs, TicketRelation, prove_ticket, verify_ticket};
use foculus::tickets::{easy_target, grind_fold, grind_settlement, self_fold};
use foculus::{CyberlinkRecord, PayStatement, Signal, TipProver, prove_pay, verify_fold_ticket, verify_pay};
use nebu::Goldilocks as F;
use tru::{Context, FocusingParams, Fx, Link};
use zheng::envelope::{Envelope, SuccinctStatement};
use zheng::execution::ExecutionNoun as N;
use zheng::execution::succinct::{ProvingKey, device_params, prove_with_key};
use zheng::types::{CCSInstance, CCSWitness};

fn reps() -> usize {
    std::env::var("DP_REPS").ok().and_then(|s| s.parse().ok()).unwrap_or(5)
}

/// Median wall time of `f` in ms over [`reps`] runs, and its last result.
fn time<T>(mut f: impl FnMut() -> T) -> (f64, T) {
    let mut ts = Vec::new();
    let mut out = None;
    for _ in 0..reps() {
        let t = Instant::now();
        out = Some(f());
        ts.push(t.elapsed().as_secs_f64() * 1e3);
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (ts[ts.len() / 2], out.unwrap())
}

fn h(i: u64) -> [u8; 32] {
    let mut x = [0u8; 32];
    x[..8].copy_from_slice(&i.to_le_bytes());
    x[31] = 0xAA;
    x
}

fn pay_signal(legs: usize) -> Signal {
    Signal {
        neuron: h(1),
        network: [0; 32],
        links: (0..legs)
            .map(|i| CyberlinkRecord {
                neuron: h(1),
                from: h(1),
                to: h(2 + i as u64),
                token: h(9),
                amount: 1_000_000 + i as u64,
                valence: 1,
                height: 0,
            })
            .collect(),
        delta_pi: vec![],
        box_moves: vec![],
        prev: [0; 32],
        step: 1,
        height: 0,
        proof: None,
    }
}

/// Bench-only: a relation proven as if closed (see the module doc).
struct AsClosed<'a, R>(&'a R);
impl<R: Relation> Relation for AsClosed<'_, R> {
    fn statement(&self) -> [u8; 32] {
        self.0.statement()
    }
    fn instance(&self) -> CCSInstance {
        self.0.instance()
    }
    fn public(&self) -> Vec<(usize, F)> {
        self.0.public()
    }
    fn closed(&self) -> bool {
        true
    }
}
impl<R: Witnessed> Witnessed for AsClosed<'_, R> {
    fn witness(&self) -> Option<CCSWitness> {
        self.0.witness()
    }
}

fn profile_one<R: Witnessed>(r: &R) -> (f64, f64, f64, usize, usize) {
    let (cold, _) = time(|| {
        let key = RelationKey::new(r).unwrap();
        proof::prove_succinct(&key, r).unwrap()
    });
    let key = RelationKey::new(r).unwrap();
    let (warm, p) = time(|| proof::prove_succinct(&key, r).unwrap());
    let (verify, ok) = time(|| proof::verify(r, &p));
    ok.unwrap();
    (cold, warm, verify, p.wire_size(), key.vars())
}

// ── nox: one hemera hash of `n` public inputs (zheng tests/common) ──────

fn pair(a: N, b: N) -> N {
    N::Pair(Box::new(a), Box::new(b))
}
fn op(tag: u64, arg: N) -> N {
    pair(N::Atom(tag), arg)
}
fn input_axis(k: usize) -> N {
    op(0, N::Atom((1u64 << (k + 2)) - 2))
}
fn hash_inputs(n: usize) -> N {
    let mut it = (0..n).map(input_axis).collect::<Vec<_>>().into_iter().rev();
    let mut acc = it.next().unwrap();
    for x in it {
        acc = op(3, pair(x, acc));
    }
    op(15, acc)
}

fn nox_hash_row(name: &str, n: usize, native_us: f64) {
    let program = hash_inputs(n);
    let input: Vec<u64> = (0..n as u64).map(|i| 1000 + 7 * i).collect();
    let budget = 1 << 40;
    let (cold, _) = time(|| {
        let key = ProvingKey::new(&program, n).unwrap();
        prove_with_key(&key, &program, &input, budget, device_params).unwrap()
    });
    let key = ProvingKey::new(&program, n).unwrap();
    let (warm, (statement, proof)) = time(|| prove_with_key(&key, &program, &input, budget, device_params).unwrap());
    let envelope = Envelope::Succinct { statement: SuccinctStatement::Execution(statement), proof };
    let bytes = envelope.to_bytes();
    let (verify, ok) = time(|| Envelope::from_bytes(&bytes).unwrap().verify(None));
    ok.unwrap();
    println!(
        "{name} | native {native_us:.2} us, 0 B | — | p1 (nox hash of {n}, 2^{}) cold {cold:.1} · warm {warm:.1} · verify {verify:.1} · {} B",
        key.vars(),
        bytes.len()
    );
}

fn main() {
    let backend = std::env::var("ZHENG_BACKEND").unwrap_or_else(|_| "cpu".into());
    let (name, device) = backend
        .parse::<cyber_lens_gpu::Choice>()
        .and_then(cyber_lens_gpu::install)
        .unwrap_or_else(|e| panic!("ZHENG_BACKEND: {e}"));
    println!("backend: {name} ({device}), reps {}", reps());

    println!("\n## pay σ (closed relation: legs pinned from the signal)");
    println!("legs | native recompute us | v3 prove · verify ms · bytes | p1 vars | p1 cold · warm · verify ms · bytes");
    for legs in [1usize, 2, 16, 64] {
        let sig = pay_signal(legs);
        let (native, _) = time(|| PayStatement::of(&sig).unwrap());
        let (v3p, p) = time(|| prove_pay(&sig).unwrap());
        let (v3v, ok) = time(|| verify_pay(&p, &sig));
        assert!(ok);
        let r = PayRelation::of(&sig).unwrap();
        let (cold, warm, verify, bytes, vars) = profile_one(&r);
        println!(
            "{legs} | {:.2} | {v3p:.3} · {v3v:.3} · {} | {vars} | {cold:.1} · {warm:.1} · {verify:.1} · {bytes}",
            native * 1e3,
            p.wire_size()
        );
    }

    println!("\n## settlement ticket (relation not closed: impulse checked natively)");
    println!("n | base links | v3 prove · verify ms · bytes | relation alone in p1 (unsound, cost only): cold · warm · verify ms · bytes");
    let (beacon, cluster) = (h(0xBE), h(0xC1));
    for (particles, n) in [(3u64, 3u64), (256, 16)] {
        let mut base = Vec::new();
        for i in 0..particles {
            base.push(Link::stake(h(i), h((i + 1) % particles), 100));
            base.push(Link::stake(h(i), h((i * 7 + 3) % particles), 40));
        }
        let contribs: Vec<Contribution> = (0..n)
            .map(|k| Contribution {
                neuron: h(1_000_000 + k),
                links: vec![Link::stake(h((k * 13 + 1) % particles), h((k * 5) % particles), 5000 + 100 * k as u128)],
                surprise: Fx::ONE,
            })
            .collect();
        let (ctx, params) = (Context::none(), FocusingParams::default());
        let inputs = TicketInputs { base: &base, contribs: &contribs, ctx: &ctx, params: &params };
        let t = grind_settlement(&base, &contribs, &ctx, &params, &beacon, &cluster, &h(7), 0, 4, 1, easy_target()).remove(0);
        let (v3p, p) = time(|| prove_ticket(inputs, &beacon, &cluster, easy_target(), &t).unwrap());
        let (v3v, ok) = time(|| verify_ticket(inputs, &beacon, &cluster, easy_target(), &p));
        ok.unwrap();
        let rel = TicketRelation::new(inputs, &beacon, &cluster, &t);
        let (cold, warm, verify, bytes, _) = profile_one(&AsClosed(&rel));
        println!(
            "{n} | {} | {v3p:.3} · {v3v:.3} · {} | {cold:.1} · {warm:.1} · {verify:.1} · {bytes}",
            base.len(),
            p.proof.wire_size()
        );
    }

    println!("\n## settlement seal (4 tickets) and fold-tree merge, tip step");
    println!("statement | native | v3 | profile-1 alternative");
    {
        let particles = 3u64;
        let mut base = Vec::new();
        for i in 0..particles {
            base.push(Link::stake(h(i), h((i + 1) % particles), 100));
        }
        let contribs: Vec<Contribution> = (0..3)
            .map(|k| Contribution {
                neuron: h(1_000_000 + k),
                links: vec![Link::stake(h(k % particles), h((k + 2) % particles), 5000)],
                surprise: Fx::ONE,
            })
            .collect();
        let (ctx, params) = (Context::none(), FocusingParams::default());
        let inputs = TicketInputs { base: &base, contribs: &contribs, ctx: &ctx, params: &params };
        let all = grind_settlement(&base, &contribs, &ctx, &params, &beacon, &cluster, &h(7), 0, 8, 8, easy_target());
        let proven: Vec<_> = all[..4].iter().map(|t| prove_ticket(inputs, &beacon, &cluster, easy_target(), t).unwrap()).collect();
        let seal = foculus::ClusterProof { target: easy_target(), tickets: proven };
        let (v, acc) = time(|| foculus::verify_cluster(inputs, &beacon, &cluster, &seal).unwrap());
        let bytes: usize = seal.tickets.iter().map(|t| t.proof.wire_size()).sum();
        println!("settlement seal, 4 tickets (n=3) | verify {v:.3} ms (impulse ×4 + hashes) | {bytes} B | see ticket rows ×4");
        let left = self_fold(3, &all[..4]);
        let right = self_fold(3, &all[4..8]);
        let ft = grind_fold(&beacon, &cluster, &h(7), 1, &left, &right, 0, 64, easy_target()).unwrap();
        let (fv, ok) = time(|| verify_fold_ticket(&beacon, &cluster, easy_target(), &ft));
        assert!(ok && acc.k == 4);
        nox_hash_row("fold-tree merge", 12, fv * 1e3);
    }
    {
        let steps = 10_000u64;
        let mut prover = TipProver::new();
        for i in 0..steps {
            prover.push_height(i, h(i)).unwrap();
        }
        let (join, tip) = time(|| foculus::Tip::join_chain(prover.chain(), &prover.head()));
        assert!(tip.grade4());
        nox_hash_row("tip step", 13, join * 1e3 / steps as f64);
    }
}
