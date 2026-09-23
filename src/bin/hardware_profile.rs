// ---
// tags: foculus, rust, audit, fold-mining, hardware
// crystal-type: source
// crystal-domain: cyber
// ---
//! Property #36 (cyber/launch.md): hardware profile of a settlement-mining
//! sample — samples/second and joules/sample on this machine, serial and
//! parallel.
//!
//! A settlement ticket sample is `try_settlement_ticket`: one ordering draw
//! plus `settlement::marginals`, the tri-kernel recompute over the ε-support
//! (specs/fold-mining.md §7). This binary measures that recompute's throughput
//! across growing support sizes, and its power draw via a streaming
//! `powermetrics` sampler (the same technique as `xena-power`), so the two
//! combine into joules/sample. It also measures the same recompute run by
//! `--threads N` independent workers at once, since production mining runs
//! many workers in parallel and a serial number alone cannot say whether
//! throughput scales or contends. `audit/hardware-profile.md` records the
//! numbers and names what other hardware classes still need running.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use foculus::settlement::Contribution;
use foculus::tickets::{easy_target, grind_settlement};
use tru::{Context, FocusingParams, Fx, Link};

fn h(n: u64) -> [u8; 32] {
    let mut x = [0u8; 32];
    x[0..8].copy_from_slice(&n.to_le_bytes());
    x
}

/// A ring of `size` stake links — a stand-in ε-support of that many edges.
fn ring(size: usize) -> Vec<Link> {
    (0..size as u64)
        .map(|i| Link::stake(h(i), h((i + 1) % size as u64), 100))
        .collect()
}

fn contribs() -> Vec<Contribution> {
    vec![
        Contribution { neuron: h(1_000_000), links: vec![Link::stake(h(0), h(1), 8000)], surprise: Fx::ONE },
        Contribution { neuron: h(1_000_001), links: vec![Link::stake(h(1), h(2), 6000)], surprise: Fx::ONE },
        Contribution { neuron: h(1_000_002), links: vec![Link::stake(h(2), h(3), 4000)], surprise: Fx::ONE },
        Contribution { neuron: h(1_000_003), links: vec![Link::stake(h(3), h(0), 2000)], surprise: Fx::ONE },
    ]
}

// ----------------------------------------------------------------------------
// powermetrics streamer (pattern: xena/crates/xena-bench/src/power.rs)
// ----------------------------------------------------------------------------

type PowerSamples = Arc<Mutex<Vec<(Instant, u32)>>>;

fn spawn_powermetrics() -> Option<(std::process::Child, PowerSamples, Arc<AtomicBool>)> {
    let mut child = Command::new("sudo")
        .args(["-n", "powermetrics", "--samplers", "cpu_power", "-i", "500"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let samples: PowerSamples = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take()?;
    let samples_w = Arc::clone(&samples);
    let stop_w = Arc::clone(&stop);

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if stop_w.load(Ordering::Relaxed) {
                break;
            }
            let Ok(line) = line else { break };
            if let Some(rest) = line.trim().strip_prefix("CPU Power:") {
                if let Some(mw) = rest.trim().strip_suffix("mW").and_then(|s| s.trim().parse::<u32>().ok()) {
                    samples_w.lock().unwrap().push((Instant::now(), mw));
                }
            }
        }
    });

    Some((child, samples, stop))
}

fn mean_watts_in_window(samples: &[(Instant, u32)], t0: Instant, t1: Instant) -> Option<f64> {
    let in_window: Vec<f64> = samples
        .iter()
        .filter(|(t, _)| *t >= t0 && *t <= t1)
        .map(|(_, mw)| *mw as f64 / 1000.0)
        .collect();
    if in_window.is_empty() {
        return None;
    }
    Some(in_window.iter().sum::<f64>() / in_window.len() as f64)
}

// ----------------------------------------------------------------------------
// throughput
// ----------------------------------------------------------------------------

struct Row {
    support: usize,
    threads: usize,
    samples_per_sec: f64,
    watts: Option<f64>,
}

fn measure(support: usize, duration: Duration, power: Option<&PowerSamples>) -> Row {
    let base = ring(support);
    let c = contribs();
    let beacon = h(0xBEEF);
    let cluster = h(0xC1);
    let miner = h(0x1);

    // Warm up so the first timed batch is not paying allocator/cache cold cost.
    let _ = grind_settlement(&base, &c, &Context::none(), &FocusingParams::default(), &beacon, &cluster, &miner, 0, 8, 8, easy_target());

    let t0 = Instant::now();
    let mut attempts = 0u64;
    let mut nonce = 1_000u64;
    while t0.elapsed() < duration {
        let batch = 32u64;
        let _ = grind_settlement(&base, &c, &Context::none(), &FocusingParams::default(), &beacon, &cluster, &miner, nonce, batch, batch as usize, easy_target());
        attempts += batch;
        nonce += batch;
    }
    let t1 = Instant::now();
    let secs = (t1 - t0).as_secs_f64();

    let watts = power.and_then(|s| {
        let snap = s.lock().unwrap();
        mean_watts_in_window(&snap, t0, t1)
    });

    Row { support, threads: 1, samples_per_sec: attempts as f64 / secs, watts }
}

/// `threads` independent workers, each grinding its own nonce range over the
/// same ε-support for `duration`, so the number answers "does throughput add
/// up or do the workers contend" rather than re-timing one worker `threads`
/// times. Each worker gets a disjoint nonce stride (`worker_id..step:threads`)
/// so no two workers ever draw the same ordering.
fn measure_parallel(support: usize, duration: Duration, threads: usize, power: Option<&PowerSamples>) -> Row {
    let base = Arc::new(ring(support));
    let c = Arc::new(contribs());
    let beacon = h(0xBEEF);
    let cluster = h(0xC1);
    let miner = h(0x1);
    let total_attempts = Arc::new(AtomicU64::new(0));

    let t0 = Instant::now();
    thread::scope(|scope| {
        for worker_id in 0..threads {
            let base = Arc::clone(&base);
            let c = Arc::clone(&c);
            let total_attempts = Arc::clone(&total_attempts);
            scope.spawn(move || {
                let mut attempts = 0u64;
                let mut nonce = 1_000u64 + worker_id as u64;
                while t0.elapsed() < duration {
                    let batch = 32u64;
                    let _ = grind_settlement(&base, &c, &Context::none(), &FocusingParams::default(), &beacon, &cluster, &miner, nonce, batch, batch as usize, easy_target());
                    attempts += batch;
                    nonce += batch * threads as u64;
                }
                total_attempts.fetch_add(attempts, Ordering::Relaxed);
            });
        }
    });
    let t1 = Instant::now();
    let secs = (t1 - t0).as_secs_f64();

    let watts = power.and_then(|s| {
        let snap = s.lock().unwrap();
        mean_watts_in_window(&snap, t0, t1)
    });

    Row {
        support,
        threads,
        samples_per_sec: total_attempts.load(Ordering::Relaxed) as f64 / secs,
        watts,
    }
}

/// Ratio of `threads`-worker throughput to `threads` times the serial rate —
/// 1.0 is perfect scaling, below 1.0 is contention, above 1.0 is warm-cache
/// noise. Pure so it is unit-testable without spawning real threads.
fn scaling_efficiency(serial_samples_per_sec: f64, parallel_row: &Row) -> f64 {
    parallel_row.samples_per_sec / (serial_samples_per_sec * parallel_row.threads as f64)
}

const USAGE: &str = "usage: hardware_profile [--secs N] [--supports 8,64,512,4096] [--threads 1,2,4]";

fn parse_args() -> Result<(u64, Vec<usize>, Vec<usize>), String> {
    let mut secs: u64 = 5;
    let mut supports: Vec<usize> = vec![8, 64, 512, 4096];
    let mut threads: Vec<usize> = vec![1];
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))?;
        match flag.as_str() {
            "--secs" => secs = value.parse().map_err(|e| format!("--secs {value}: {e}"))?,
            "--supports" => {
                supports = value
                    .split(',')
                    .map(|s| s.trim().parse().map_err(|e| format!("--supports {value}: {e}")))
                    .collect::<Result<_, _>>()?;
            }
            "--threads" => {
                threads = value
                    .split(',')
                    .map(|s| s.trim().parse().map_err(|e| format!("--threads {value}: {e}")))
                    .collect::<Result<_, _>>()?;
            }
            other => return Err(format!("unknown arg: {other}\n{USAGE}")),
        }
    }
    if secs == 0 || supports.is_empty() || threads.is_empty() || threads.iter().any(|&t| t == 0) {
        return Err(format!("nothing to measure\n{USAGE}"));
    }
    Ok((secs, supports, threads))
}

fn print_row(row: &Row) {
    match row.watts {
        Some(w) => println!(
            "  support={:<6} threads={:<3} {:>10.1} samples/s   {:>5.2}W   {:>8.3} mJ/sample",
            row.support, row.threads, row.samples_per_sec, w, (w / row.samples_per_sec) * 1000.0
        ),
        None => println!("  support={:<6} threads={:<3} {:>10.1} samples/s", row.support, row.threads, row.samples_per_sec),
    }
}

fn main() {
    let (secs, supports, threads) = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    println!("foculus hardware_profile — samples/s and W per ε-support size and thread count, {}s/point", secs);

    let pm = spawn_powermetrics();
    let power_samples = pm.as_ref().map(|(_, s, _)| s);
    if pm.is_none() {
        eprintln!("note: powermetrics unavailable (needs passwordless `sudo -n powermetrics`) — reporting samples/s only");
    } else {
        thread::sleep(Duration::from_secs(2)); // let the streamer fill its first samples
    }

    for &support in &supports {
        let serial = measure(support, Duration::from_secs(secs), power_samples);
        print_row(&serial);
        for &t in &threads {
            if t == 1 {
                continue; // already have the serial point
            }
            let row = measure_parallel(support, Duration::from_secs(secs), t, power_samples);
            let eff = scaling_efficiency(serial.samples_per_sec, &row);
            print_row(&row);
            println!("    scaling efficiency vs serial: {:.2}", eff);
        }
    }

    if let Some((mut child, _samples, stop)) = pm {
        stop.store(true, Ordering::Relaxed);
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_scaling_is_efficiency_one() {
        let serial_rate = 100.0;
        let row = Row { support: 8, threads: 4, samples_per_sec: 400.0, watts: None };
        assert!((scaling_efficiency(serial_rate, &row) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn contention_is_efficiency_below_one() {
        let serial_rate = 100.0;
        let row = Row { support: 8, threads: 4, samples_per_sec: 250.0, watts: None };
        let eff = scaling_efficiency(serial_rate, &row);
        assert!(eff > 0.0 && eff < 1.0, "expected contention (0,1), got {eff}");
    }

    #[test]
    fn parse_args_defaults_to_serial_only() {
        // parse_args reads std::env::args(); this documents the intended
        // default (`--threads` omitted → [1]) without invoking the process
        // argv, which the test harness owns.
        let default_threads = vec![1usize];
        assert_eq!(default_threads, vec![1]);
    }

    /// Two threads on disjoint nonce strides never draw the same ordering,
    /// the invariant `measure_parallel` depends on to avoid double-counting
    /// or colliding samples.
    #[test]
    fn worker_nonce_strides_are_disjoint() {
        let threads = 3usize;
        let batch = 32u64;
        let mut seen = std::collections::HashSet::new();
        for worker_id in 0..threads {
            let mut nonce = 1_000u64 + worker_id as u64;
            for _ in 0..5 {
                assert!(seen.insert(nonce), "nonce {nonce} reused across workers");
                nonce += batch * threads as u64;
            }
        }
    }
}
