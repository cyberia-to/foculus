// ---
// tags: foculus, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! The `foculus` command line — the consensus engine, driveable by hand.
//!
//! Core commands need no network: they run the tri-kernel, fork-choice, finality
//! gate, and settlement lottery over inputs given on the command line. Particles
//! are named by label — any string, hashed to its particle id via hemera — so a
//! link reads `alice:bob:100` and results print back by label. The device-sync
//! daemon lives under `node`, behind the `net` feature.

use std::collections::BTreeMap;

use anyhow::{bail, Context as _, Result};
use clap::{Parser, Subcommand};

use tru::{compute_focusing, Context, FocusingGraph, FocusingParams, Fx, Link};

use crate::chain::{CyberlinkRecord, Signal, SELF_NETWORK};
use crate::finality::{certified, crosses_threshold, finalizes, Domain, Finality};
use crate::focus::Focus;
use crate::fork::{ForkChoice, MinHash, Serialize};
use crate::reconcile::Reconciler;
use crate::settlement::{self, Contribution};

#[derive(Parser)]
#[command(
    name = "foculus",
    about = "consensus by convergence — φ*, fork-choice, finality, settlement, sync",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compute the φ* focus distribution over a set of cyberlinks (the tri-kernel).
    Focus {
        /// A stake-weighted cyberlink `from:to:amount`. Repeatable.
        #[arg(long = "link", value_name = "FROM:TO:AMOUNT", required = true)]
        links: Vec<String>,
    },

    /// Detect conflicts among signals and resolve them by a fork-choice strategy.
    Reconcile {
        /// `serialize` (single-writer), `minhash` (trusted), or `focus` (φ*).
        #[arg(long, default_value = "minhash")]
        strategy: String,
        /// A single-link signal `neuron:step:from:to`. Repeatable.
        #[arg(long = "signal", value_name = "NEURON:STEP:FROM:TO", required = true)]
        signals: Vec<String>,
    },

    /// List detected conflicts among signals (detection only, no resolution).
    Conflicts {
        #[arg(long = "signal", value_name = "NEURON:STEP:FROM:TO", required = true)]
        signals: Vec<String>,
    },

    /// Decide whether a particle finalizes (adaptive threshold + certification gate).
    Finality {
        /// A φ* value in the candidate's ε-support domain. Repeatable — the domain.
        #[arg(long = "phi", value_name = "VALUE", required = true)]
        phi: Vec<String>,
        /// The candidate particle's own φ*.
        #[arg(long)]
        target: String,
        /// Adaptive-threshold multiplier κ' ∈ [1,2].
        #[arg(long = "kappa-prime", default_value = "1.5")]
        kappa_prime: String,
        /// Uncertified φ*-mass in the domain (Φ_uncert).
        #[arg(long, default_value = "0")]
        uncert: String,
        /// φ*-gap between winner and runner-up (Δ_D).
        #[arg(long, default_value = "0.085")]
        gap: String,
        /// Domain-local contraction rate κ_D.
        #[arg(long = "kappa-d", default_value = "0.74")]
        kappa_d: String,
        /// Tri-kernel Lipschitz constant C.
        #[arg(long, default_value = "2.25")]
        c: String,
    },

    /// Run the settlement Shapley lottery (beacon-seeded Monte-Carlo).
    Settle {
        /// A base-graph cyberlink `from:to:amount`. Repeatable.
        #[arg(long = "link", value_name = "FROM:TO:AMOUNT", required = true)]
        base: Vec<String>,
        /// A contributor's link `neuron:from:to:amount`. Repeatable.
        #[arg(long = "contrib", value_name = "NEURON:FROM:TO:AMOUNT", required = true)]
        contribs: Vec<String>,
        /// Monte-Carlo samples (more = tighter estimate).
        #[arg(long, default_value = "64")]
        samples: u64,
        /// Epoch beacon label (seeds the orderings).
        #[arg(long, default_value = "beacon")]
        beacon: String,
    },

    /// Verify a signal chain and report equivocations.
    Chain {
        #[arg(long = "signal", value_name = "NEURON:STEP:FROM:TO", required = true)]
        signals: Vec<String>,
    },

    /// Device sync daemon — erasure-coded storage over iroh QUIC (`net` feature).
    #[cfg(feature = "net")]
    Node(NodeArgs),
}

/// Entry point — `fn main` in the bin is a one-liner over this.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Focus { links } => cmd_focus(&links),
        Command::Reconcile { strategy, signals } => cmd_reconcile(&strategy, &signals),
        Command::Conflicts { signals } => cmd_conflicts(&signals),
        Command::Finality {
            phi,
            target,
            kappa_prime,
            uncert,
            gap,
            kappa_d,
            c,
        } => cmd_finality(&phi, &target, &kappa_prime, &uncert, &gap, &kappa_d, &c),
        Command::Settle {
            base,
            contribs,
            samples,
            beacon,
        } => cmd_settle(&base, &contribs, samples, &beacon),
        Command::Chain { signals } => cmd_chain(&signals),
        #[cfg(feature = "net")]
        Command::Node(args) => node::run(args),
    }
}

// ── labels ────────────────────────────────────────────────────────────────

/// A label registry: intern strings to particle ids (hemera hash) and reverse
/// them for pretty output.
#[derive(Default)]
struct Labels {
    to_id: BTreeMap<String, [u8; 32]>,
    to_label: BTreeMap<[u8; 32], String>,
}

impl Labels {
    fn id(&mut self, label: &str) -> [u8; 32] {
        if let Some(id) = self.to_id.get(label) {
            return *id;
        }
        let id = *cyber_hemera::hash(label.as_bytes())
            .as_bytes()
            .first_chunk::<32>()
            .unwrap_or(&[0u8; 32]);
        self.to_id.insert(label.to_string(), id);
        self.to_label.insert(id, label.to_string());
        id
    }

    fn label(&self, id: &[u8; 32]) -> String {
        self.to_label
            .get(id)
            .cloned()
            .unwrap_or_else(|| short_hex(id))
    }
}

fn short_hex(id: &[u8; 32]) -> String {
    let mut s = String::with_capacity(12);
    for b in &id[..6] {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

// ── parsing ─────────────────────────────────────────────────────────────

/// Parse a decimal like "0.85", "1.5", "2" into an exact `Fx` — no float on the
/// deterministic path; the decimal is read as a rational num/den.
fn parse_fx(s: &str) -> Result<Fx> {
    let s = s.trim();
    if let Some((int, frac)) = s.split_once('.') {
        let den: i64 = 10i64
            .checked_pow(frac.len() as u32)
            .context("fractional part too long")?;
        let int_v: i64 = if int.is_empty() || int == "-" {
            0
        } else {
            int.parse().context("bad integer part")?
        };
        let frac_v: i64 = if frac.is_empty() {
            0
        } else {
            frac.parse().context("bad fractional part")?
        };
        let sign = if s.starts_with('-') { -1 } else { 1 };
        let num = int_v.abs() * den + frac_v;
        Ok(Fx::from_int(sign * num).div(Fx::from_int(den)))
    } else {
        Ok(Fx::from_int(s.parse().context("bad integer")?))
    }
}

/// `from:to:amount` → a stake-weighted cyberlink.
fn parse_link(spec: &str, labels: &mut Labels) -> Result<CyberlinkRecord> {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() != 3 {
        bail!("link must be from:to:amount, got `{spec}`");
    }
    let amount: u64 = parts[2].parse().context("bad amount")?;
    let from = labels.id(parts[0]);
    let to = labels.id(parts[1]);
    Ok(CyberlinkRecord {
        neuron: from,
        from,
        to,
        token: [0u8; 32],
        amount,
        valence: 1,
        height: 0,
    })
}

/// `neuron:step:from:to` → a single-link signal.
fn parse_signal(spec: &str, labels: &mut Labels) -> Result<Signal> {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() != 4 {
        bail!("signal must be neuron:step:from:to, got `{spec}`");
    }
    let neuron = labels.id(parts[0]);
    let step: u64 = parts[1].parse().context("bad step")?;
    let from = labels.id(parts[2]);
    let to = labels.id(parts[3]);
    Ok(Signal {
        neuron,
        network: SELF_NETWORK,
        links: vec![CyberlinkRecord {
            neuron,
            from,
            to,
            token: [0u8; 32],
            amount: 1,
            valence: 1,
            height: 0,
        }],
        delta_pi: vec![],
        prev: [0u8; 32],
        step,
        height: 0,
        proof: None,
    })
}

// ── commands ────────────────────────────────────────────────────────────

fn cmd_focus(link_specs: &[String]) -> Result<()> {
    let mut labels = Labels::default();
    let mut links = Vec::new();
    for s in link_specs {
        let l = parse_link(s, &mut labels)?;
        links.push(Link::stake(l.from, l.to, l.amount as u128));
    }
    let ctx = Context::none();
    let graph = FocusingGraph::build(links, &ctx);
    let params = FocusingParams::default();
    let result = compute_focusing(&graph, &params);

    let ids = graph.node_ids();
    let mut rows: Vec<(String, Fx)> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (labels.label(id), result.focus[i]))
        .collect();
    // highest focus first; label as deterministic tiebreak
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    println!("φ* over {} particles:", rows.len());
    for (label, phi) in &rows {
        println!("  {:<16} {:.6}", label, phi.to_f64());
    }
    println!("syntropy J(φ*) = {:.6}", result.syntropy.to_f64());
    Ok(())
}

fn cmd_reconcile(strategy: &str, signal_specs: &[String]) -> Result<()> {
    let mut labels = Labels::default();
    let signals: Vec<Signal> = signal_specs
        .iter()
        .map(|s| parse_signal(s, &mut labels))
        .collect::<Result<_>>()?;

    // one engine per strategy; the resolved winners come back the same way.
    let resolved = match strategy {
        "serialize" => run_reconcile(Serialize, &signals),
        "minhash" => run_reconcile(MinHash, &signals),
        "focus" => run_reconcile(Focus::new(), &signals),
        other => bail!("unknown strategy `{other}` (serialize | minhash | focus)"),
    }?;

    if resolved.is_empty() {
        println!("no conflicts among {} signals — nothing to resolve", signals.len());
        return Ok(());
    }
    println!("resolved {} conflict(s) with strategy `{strategy}`:", resolved.len());
    for r in &resolved {
        // find the winning signal to print its neuron:step
        let who = signals
            .iter()
            .find(|s| s.content_id() == r.winner)
            .map(|s| format!("{}:{}", labels.label(&s.neuron), s.step))
            .unwrap_or_else(|| short_hex(&r.winner));
        println!("  conflict {} → winner {who}", short_hex(&r.key));
    }
    Ok(())
}

fn run_reconcile<F: ForkChoice>(
    fork: F,
    signals: &[Signal],
) -> Result<Vec<crate::reconcile::Resolved>> {
    let mut rec = Reconciler::new(fork);
    let mut out = Vec::new();
    for s in signals {
        match rec.observe(s) {
            Ok(mut r) => out.append(&mut r),
            Err(e) => bail!("reconcile failed: {e:?}"),
        }
    }
    Ok(out)
}

fn cmd_conflicts(signal_specs: &[String]) -> Result<()> {
    let mut labels = Labels::default();
    let mut index = crate::conflict::ConflictIndex::new();
    let mut signals = Vec::new();
    for s in signal_specs {
        let sig = parse_signal(s, &mut labels)?;
        index.observe(&sig);
        signals.push(sig);
    }
    let conflicts = index.conflicts();
    if conflicts.is_empty() {
        println!("no conflicts among {} signals", signals.len());
        return Ok(());
    }
    println!("{} conflict group(s):", conflicts.len());
    for key in &conflicts {
        let group = index.group(key).unwrap();
        let members: Vec<String> = group
            .members()
            .iter()
            .map(|s| format!("{}:{}", labels.label(&s.neuron), s.step))
            .collect();
        println!("  {} : {}", short_hex(key), members.join(" ⟂ "));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_finality(
    phi: &[String],
    target: &str,
    kappa_prime: &str,
    uncert: &str,
    gap: &str,
    kappa_d: &str,
    c: &str,
) -> Result<()> {
    let focus: Vec<Fx> = phi.iter().map(|s| parse_fx(s)).collect::<Result<_>>()?;
    let particles: Vec<[u8; 32]> = (0..focus.len() as u8).map(|b| [b; 32]).collect();
    let domain = Domain::from_focus(particles, focus);
    let target = parse_fx(target)?;
    let kp = parse_fx(kappa_prime)?;
    let uncert = parse_fx(uncert)?;
    let gap = parse_fx(gap)?;
    let kd = parse_fx(kappa_d)?;
    let c = parse_fx(c)?;

    let crosses = crosses_threshold(target, &domain, kp);
    let cert = certified(uncert, gap, kd, c, kp);
    let verdict = finalizes(target, &domain, uncert, gap, kd, c, kp);

    println!("domain: {} particles", domain.len());
    println!("  μ_D              = {:.6}", domain.mean().to_f64());
    println!("  var_D            = {:.6}", domain.variance().to_f64());
    println!("  φ*_target        = {:.6}", target.to_f64());
    println!("  crosses τ_D      = {}", crosses);
    println!("  certified (P2)   = {}", cert);
    println!(
        "  verdict          = {}",
        match verdict {
            Finality::Final => "FINAL",
            Finality::Pending => "pending",
        }
    );
    Ok(())
}

fn cmd_settle(base_specs: &[String], contrib_specs: &[String], samples: u64, beacon: &str) -> Result<()> {
    let mut labels = Labels::default();
    let mut base = Vec::new();
    for s in base_specs {
        let l = parse_link(s, &mut labels)?;
        base.push(Link::stake(l.from, l.to, l.amount as u128));
    }
    let mut contribs = Vec::new();
    for s in contrib_specs {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 4 {
            bail!("contrib must be neuron:from:to:amount, got `{s}`");
        }
        let neuron = labels.id(parts[0]);
        let from = labels.id(parts[1]);
        let to = labels.id(parts[2]);
        let amount: u64 = parts[3].parse().context("bad amount")?;
        contribs.push(Contribution {
            neuron,
            links: vec![Link::stake(from, to, amount as u128)],
            surprise: Fx::ONE,
        });
    }
    let beacon_id = labels.id(beacon);
    let ctx = Context::none();
    let params = FocusingParams::default();
    let shares = settlement::shapley(&base, &contribs, &ctx, &params, samples, &beacon_id);

    println!("settlement over {samples} beacon-seeded samples:");
    let total: f64 = shares.iter().map(|(_, s)| s.to_f64()).sum();
    for (neuron, share) in &shares {
        println!("  {:<16} {:.6}", labels.label(neuron), share.to_f64());
    }
    println!("Σ shares = {:.6}", total);
    Ok(())
}

fn cmd_chain(signal_specs: &[String]) -> Result<()> {
    use crate::chain::{ChainError, SignalChain};
    let mut labels = Labels::default();
    // group signals per neuron, append in step order, report the outcome
    let mut chains: BTreeMap<[u8; 32], SignalChain> = BTreeMap::new();
    let mut equivocations = 0usize;
    let mut ok = 0usize;
    for spec in signal_specs {
        let sig = parse_signal(spec, &mut labels)?;
        let neuron = sig.neuron;
        let chain = chains.entry(neuron).or_default();
        match chain.append(sig) {
            Ok(()) => ok += 1,
            Err(ChainError::Equivocation) => {
                equivocations += 1;
                println!("  equivocation: {}", short_hex(&neuron));
            }
            Err(e) => println!("  chain error ({}): {e:?}", short_hex(&neuron)),
        }
    }
    println!(
        "{} chain(s): {ok} appended, {equivocations} equivocation(s)",
        chains.len()
    );
    Ok(())
}

// ── the network daemon, behind `net` ────────────────────────────────────

#[cfg(feature = "net")]
mod node {
    use std::path::PathBuf;

    use anyhow::Result;
    use clap::{Args, Subcommand};

    use crate::node::SyncNode;

    #[derive(Args)]
    pub struct NodeArgs {
        /// Data directory for this device.
        #[arg(short, long, default_value = "~/.foculus")]
        dir: String,
        /// QUIC port (fixed for stable address).
        #[arg(short, long, default_value = "4200")]
        port: u16,
        /// Data shards (k).
        #[arg(short, long, default_value = "2")]
        k: usize,
        /// Total shards (n). Power of 2.
        #[arg(short, long, default_value = "4")]
        n: usize,
        #[command(subcommand)]
        cmd: NodeCommand,
    }

    #[derive(Subcommand)]
    enum NodeCommand {
        /// Start the sync daemon with background auto-sync.
        Daemon {
            #[arg(short, long, default_value = "30")]
            interval: u64,
        },
        /// Store a file.
        Put {
            file: PathBuf,
            #[arg(short, long)]
            name: Option<String>,
        },
        /// Retrieve a file.
        Get {
            name: String,
            #[arg(short, long)]
            output: Option<PathBuf>,
        },
        /// Delete a file (tombstone).
        Rm { name: String },
        /// List files.
        Ls,
        /// Show status.
        Status,
        /// Add a peer by node ID.
        AddPeer {
            node_id: String,
            #[arg(short, long, default_value = "0")]
            capacity: String,
        },
        /// Sync with all peers (or a specific one).
        Sync { peer: Option<String> },
        /// Garbage collect orphaned chunks.
        Gc,
        /// Integrity audit: hash-check all local chunks.
        Audit,
    }

    pub fn run(args: NodeArgs) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async move { run_async(args).await })
    }

    async fn run_async(args: NodeArgs) -> Result<()> {
        tracing_subscriber::fmt::init();
        let dir = expand_tilde(&args.dir);
        let node = SyncNode::start(&dir, args.k, args.n, args.port).await?;

        match args.cmd {
            NodeCommand::Daemon { interval } => node.run_daemon(interval).await?,
            NodeCommand::Put { file, name } => {
                let data = std::fs::read(&file)?;
                let name = name.unwrap_or_else(|| {
                    file.file_name().unwrap_or_default().to_string_lossy().to_string()
                });
                node.put_file(&name, &data).await?;
            }
            NodeCommand::Get { name, output } => {
                let data = node.get_file(&name).await?;
                if let Some(path) = output {
                    std::fs::write(&path, &data)?;
                    println!("wrote {} bytes to {}", data.len(), path.display());
                } else {
                    use std::io::Write;
                    std::io::stdout().write_all(&data)?;
                }
            }
            NodeCommand::Rm { name } => node.delete_file(&name).await?,
            NodeCommand::Ls => {
                let files = node.list_files().await;
                if files.is_empty() {
                    println!("(no files)");
                } else {
                    for f in &files {
                        println!("{f}");
                    }
                }
            }
            NodeCommand::Status => {
                let (files, peers, chunks, _) = node.status().await;
                println!("node:   {}", node.node_id());
                println!("files:  {files}");
                println!("peers:  {peers}");
                println!("chunks: {chunks}");
                println!("erasure: k={}, n={}", args.k, args.n);
            }
            NodeCommand::AddPeer { node_id, capacity } => {
                let cap = parse_capacity(&capacity)?;
                node.add_peer(&node_id, cap).await?;
            }
            NodeCommand::Sync { peer } => {
                if let Some(p) = peer {
                    node.sync_with(&p).await?;
                } else {
                    node.sync_all().await?;
                }
            }
            NodeCommand::Gc => {
                node.gc().await?;
            }
            NodeCommand::Audit => {
                node.audit().await?;
            }
        }
        node.save().await?;
        Ok(())
    }

    fn expand_tilde(path: &str) -> PathBuf {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home).join(rest);
            }
        }
        PathBuf::from(path)
    }

    fn parse_capacity(s: &str) -> Result<u64> {
        let s = s.trim().to_uppercase();
        if s == "0" || s.is_empty() {
            return Ok(0);
        }
        let (num_str, mult) = if let Some(n) = s.strip_suffix("GB") {
            (n, 1_000_000_000u64)
        } else if let Some(n) = s.strip_suffix("MB") {
            (n, 1_000_000u64)
        } else if let Some(n) = s.strip_suffix("KB") {
            (n, 1_000u64)
        } else if let Some(n) = s.strip_suffix('B') {
            (n, 1u64)
        } else {
            (s.as_str(), 1u64)
        };
        let num: u64 = num_str.trim().parse()?;
        Ok(num * mult)
    }
}

#[cfg(feature = "net")]
pub use node::NodeArgs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fx_reads_decimals_exactly() {
        assert!((parse_fx("0.85").unwrap().to_f64() - 0.85).abs() < 1e-9);
        assert!((parse_fx("1.5").unwrap().to_f64() - 1.5).abs() < 1e-9);
        assert!((parse_fx("2").unwrap().to_f64() - 2.0).abs() < 1e-9);
        assert!((parse_fx("0.003").unwrap().to_f64() - 0.003).abs() < 1e-9);
    }

    #[test]
    fn labels_are_stable_and_reversible() {
        let mut l = Labels::default();
        let a1 = l.id("alice");
        let a2 = l.id("alice");
        assert_eq!(a1, a2, "same label → same id");
        assert_eq!(l.label(&a1), "alice", "id reverses to label");
        assert_ne!(l.id("bob"), a1, "distinct labels → distinct ids");
    }

    #[test]
    fn parse_link_and_signal_shapes() {
        let mut l = Labels::default();
        let link = parse_link("alice:bob:100", &mut l).unwrap();
        assert_eq!(link.amount, 100);
        assert_eq!(link.from, l.id("alice"));
        let sig = parse_signal("alice:0:x:y", &mut l).unwrap();
        assert_eq!(sig.step, 0);
        assert_eq!(sig.links.len(), 1);
        assert!(parse_link("bad", &mut l).is_err());
        assert!(parse_signal("a:b:c", &mut l).is_err());
    }
}
