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
use std::io::{self, IsTerminal};

use anyhow::{bail, Context as _, Result};
use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, Subcommand};

use tru::{compute_focusing, Context, FocusingGraph, FocusingParams, Fx, Link};

use crate::chain::{CyberlinkRecord, Signal, SELF_NETWORK};
use crate::finality::{certified, crosses_threshold, finalizes, Domain, Finality};
use crate::focus::Focus;
use crate::fork::{ForkChoice, MinHash, Serialize};
use crate::reconcile::Reconciler;
use crate::beacon::GENESIS_PREV;
use crate::rewards::{claim_from_links, settle_epoch, verify_receipt};
use crate::settlement::{self, Contribution};

// ── color ───────────────────────────────────────────────────────────────
// ANSI only on a terminal — piped output stays clean, the cyber house style.

fn tty() -> bool {
    io::stdout().is_terminal()
}

fn paint(code: &str, s: &str) -> String {
    if tty() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn dim(s: &str) -> String {
    paint("90", s)
}
fn cyan(s: &str) -> String {
    paint("36", s)
}
fn green(s: &str) -> String {
    paint("32", s)
}
fn yellow(s: &str) -> String {
    paint("33", s)
}
fn red(s: &str) -> String {
    paint("31", s)
}
fn bold(s: &str) -> String {
    paint("1", s)
}

/// `key value`, key dimmed — the aligned kv row used across the output.
fn kv(key: &str, val: &str) -> String {
    format!("{} {}", dim(key), val)
}

/// clap's own `--help` in the cyber palette: green headers, cyan literals,
/// dim placeholders.
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default())
    .placeholder(AnsiColor::BrightBlack.on_default());

/// Wordmark + tagline — printed only on a terminal, above `--help`.
const LOGO: &str = "\
\x1b[31m███████╗ ██████╗  ██████╗██╗   ██╗██╗     ██╗   ██╗███████╗\x1b[0m
\x1b[33m██╔════╝██╔═══██╗██╔════╝██║   ██║██║     ██║   ██║██╔════╝\x1b[0m
\x1b[32m█████╗  ██║   ██║██║     ██║   ██║██║     ██║   ██║███████╗\x1b[0m
\x1b[36m██╔══╝  ██║   ██║██║     ██║   ██║██║     ██║   ██║╚════██║\x1b[0m
\x1b[34m██║     ╚██████╔╝╚██████╗╚██████╔╝███████╗╚██████╔╝███████║\x1b[0m
\x1b[35m╚═╝      ╚═════╝  ╚═════╝ ╚═════╝ ╚══════╝ ╚═════╝ ╚══════╝\x1b[0m";

fn banner() -> String {
    if !tty() {
        return String::new();
    }
    format!(
        "{LOGO}\n{tag}\n{params}\n",
        tag = paint("37", "    consensus by convergence"),
        params = dim(
            "\n    φ* · fork-choice · finality · settlement · sync\n    \
             Goldilocks field · p = 2⁶⁴ − 2³² + 1 · fixed-point\n"
        ),
    )
}

#[derive(Parser)]
#[command(
    name = "foculus",
    about = "consensus by convergence — φ*, fork-choice, finality, settlement, sync",
    version,
    styles = STYLES
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
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

    /// Run epoch settle: claims → ρ → Shapley under beacon → receipt.
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
        /// Epoch index (beacon chain position).
        #[arg(long, default_value = "1")]
        epoch: u64,
        /// Token budget to allocate across positive Shapley shares.
        #[arg(long, default_value = "1000")]
        budget: u64,
        /// Optional fixed beacon label (legacy raw lottery; skips receipt path).
        #[arg(long)]
        beacon: Option<String>,
    },

    /// Verify a signal chain and report equivocations.
    Chain {
        #[arg(long = "signal", value_name = "NEURON:STEP:FROM:TO", required = true)]
        signals: Vec<String>,
    },

    /// Device sync daemon — erasure-coded storage over iroh QUIC (`net` feature).
    #[cfg(feature = "net")]
    Node(NodeArgs),

    /// Settle-gossip radio — multi-miner SelfAcc / claims over iroh QUIC.
    #[cfg(feature = "net")]
    SettleNet(settle_net::SettleNetArgs),
}

/// Entry point — `fn main` in the bin is a one-liner over this.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        // bare `foculus` — greet, then show the command list.
        print!("{}", banner());
        let mut cmd = <Cli as clap::CommandFactory>::command();
        cmd.print_help().ok();
        return Ok(());
    };
    match command {
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
            epoch,
            budget,
            beacon,
        } => cmd_settle(&base, &contribs, samples, epoch, budget, beacon.as_deref()),
        Command::Chain { signals } => cmd_chain(&signals),
        #[cfg(feature = "net")]
        Command::Node(args) => node::run(args),
        #[cfg(feature = "net")]
        Command::SettleNet(args) => settle_net::run(args),
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
            box_moves: vec![],
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

    println!(
        "{} {}",
        green("focus"),
        dim(&format!("φ* over {} particles", rows.len()))
    );
    // a light bar so the distribution is visible at a glance
    let max = rows.first().map(|(_, p)| p.to_f64()).unwrap_or(1.0).max(1e-9);
    for (label, phi) in &rows {
        let f = phi.to_f64();
        let bar = "▮".repeat(((f / max) * 16.0).round() as usize);
        println!(
            "  {}  {}  {}",
            cyan(&format!("{label:<14}")),
            yellow(&format!("{f:.6}")),
            green(&bar)
        );
    }
    println!(
        "  {}",
        kv("syntropy", &yellow(&format!("{:.6}", result.syntropy.to_f64())))
    );
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
        println!(
            "{} {}",
            green("reconcile"),
            dim(&format!(
                "no conflicts among {} signals — nothing to resolve",
                signals.len()
            ))
        );
        return Ok(());
    }
    println!(
        "{} {}",
        green("reconcile"),
        dim(&format!(
            "{} conflict(s) · strategy {}",
            resolved.len(),
            bold(strategy)
        ))
    );
    for r in &resolved {
        // find the winning signal to print its neuron:step
        let who = signals
            .iter()
            .find(|s| s.content_id() == r.winner)
            .map(|s| format!("{}:{}", labels.label(&s.neuron), s.step))
            .unwrap_or_else(|| short_hex(&r.winner));
        println!(
            "  {} {} {}",
            dim(&short_hex(&r.key)),
            dim("→"),
            green(&format!("✓ {who}"))
        );
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
        println!(
            "{} {}",
            green("conflicts"),
            dim(&format!("none among {} signals", signals.len()))
        );
        return Ok(());
    }
    println!(
        "{} {}",
        green("conflicts"),
        dim(&format!("{} group(s)", conflicts.len()))
    );
    for key in &conflicts {
        let group = index.group(key).unwrap();
        let members: Vec<String> = group
            .members()
            .iter()
            .map(|s| yellow(&format!("{}:{}", labels.label(&s.neuron), s.step)))
            .collect();
        println!(
            "  {} {}",
            dim(&short_hex(key)),
            members.join(&dim(" ⟂ "))
        );
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

    let flag = |b: bool| {
        if b {
            green("yes")
        } else {
            red("no")
        }
    };
    let num = |x: f64| yellow(&format!("{x:.6}"));

    println!(
        "{} {}",
        green("finality"),
        dim(&format!("domain of {} particles", domain.len()))
    );
    println!("  {}", kv("μ_D          ", &num(domain.mean().to_f64())));
    println!("  {}", kv("var_D        ", &num(domain.variance().to_f64())));
    println!("  {}", kv("φ*_target    ", &num(target.to_f64())));
    println!("  {}", kv("crosses τ_D  ", &flag(crosses)));
    println!("  {}", kv("certified P2 ", &flag(cert)));
    println!(
        "  {} {}",
        dim("verdict      "),
        match verdict {
            Finality::Final => bold(&green("● FINAL")),
            Finality::Pending => yellow("○ pending"),
        }
    );
    Ok(())
}

fn cmd_settle(
    base_specs: &[String],
    contrib_specs: &[String],
    samples: u64,
    epoch: u64,
    budget: u64,
    beacon_label: Option<&str>,
) -> Result<()> {
    let mut labels = Labels::default();
    let mut base = Vec::new();
    for s in base_specs {
        let l = parse_link(s, &mut labels)?;
        base.push(Link::stake(l.from, l.to, l.amount as u128));
    }
    let ctx = Context::none();
    let params = FocusingParams::default();

    // Legacy path: fixed beacon label → raw Shapley only (no receipt).
    if let Some(beacon) = beacon_label {
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
        let shares = settlement::shapley(&base, &contribs, &ctx, &params, samples, &beacon_id);
        println!(
            "{} {}",
            green("settle"),
            dim(&format!("raw lottery · {samples} samples · beacon label"))
        );
        let total: f64 = shares.iter().map(|(_, s)| s.to_f64()).sum();
        for (neuron, share) in &shares {
            println!(
                "  {}  {}",
                cyan(&format!("{:<14}", labels.label(neuron))),
                yellow(&format!("{:.6}", share.to_f64()))
            );
        }
        println!("  {}", kv("Σ shares", &yellow(&format!("{total:.6}"))));
        return Ok(());
    }

    // Epoch path: RewardClaim → ρ → beacon → Shapley → SettleReceipt.
    let mut claims = Vec::new();
    for (i, s) in contrib_specs.iter().enumerate() {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 4 {
            bail!("contrib must be neuron:from:to:amount, got `{s}`");
        }
        let neuron = labels.id(parts[0]);
        let from = labels.id(parts[1]);
        let to = labels.id(parts[2]);
        let amount: u64 = parts[3].parse().context("bad amount")?;
        let mut id = [0u8; 32];
        id[0] = (i as u8).wrapping_add(1);
        id[1..9].copy_from_slice(&epoch.to_le_bytes());
        claims.push(claim_from_links(
            id,
            neuron,
            vec![Link::stake(from, to, amount as u128)],
            1,
        ));
    }
    let rec = settle_epoch(
        epoch,
        &GENESIS_PREV,
        &base,
        &claims,
        &ctx,
        &params,
        samples,
        budget,
    )
    .map_err(|e| anyhow::anyhow!("settle_epoch failed: {e:?}"))?;
    if !verify_receipt(&rec) {
        bail!("receipt hash self-check failed");
    }

    println!(
        "{} {}",
        green("settle"),
        dim(&format!(
            "epoch {epoch} · {samples} samples · budget {budget}"
        ))
    );
    println!("  {}", kv("beacon", &hex32(&rec.beacon)));
    println!("  {}", kv("claims_root", &hex32(&rec.claims_root)));
    println!("  {}", kv("receipt", &hex32(&rec.receipt_hash)));
    println!(
        "  {}",
        kv(
            "Δφ⁺ total",
            &yellow(&format!("{:.6}", rec.directed_total.to_f64()))
        )
    );
    let paid: u64 = rec.shares.iter().map(|s| s.amount).sum();
    for s in &rec.shares {
        println!(
            "  {}  share={}  amount={}",
            cyan(&format!("{:<14}", labels.label(&s.neuron))),
            yellow(&format!("{:.6}", s.share.to_f64())),
            green(&s.amount.to_string())
        );
    }
    println!("  {}", kv("Σ mint", &green(&paid.to_string())));
    Ok(())
}

fn hex32(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<String>()[..16].to_string() + "…"
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
                println!(
                    "  {} {} {}",
                    red("⚠"),
                    yellow("equivocation"),
                    dim(&short_hex(&neuron))
                );
            }
            Err(e) => println!(
                "  {} {} {}",
                red("✗"),
                dim(&short_hex(&neuron)),
                dim(&format!("{e:?}"))
            ),
        }
    }
    let equiv_str = if equivocations == 0 {
        green("0 equivocations")
    } else {
        yellow(&format!("{equivocations} equivocation(s)"))
    };
    println!(
        "{} {}",
        green("chain"),
        dim(&format!(
            "{} chain(s) · {ok} appended · ",
            chains.len()
        )) + &equiv_str
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

// ── settle radio daemon ───────────────────────────────────────────────────

#[cfg(feature = "net")]
mod settle_net {
    use std::path::PathBuf;
    use std::time::Duration;

    use anyhow::{Context, Result, bail};
    use clap::{Args, Subcommand};
    use iroh::{EndpointAddr, EndpointId};

    use crate::beacon::{claims_root, open_beacon, GENESIS_PREV, TEST_OUTER_T};
    use crate::gossip::SettleMsg;
    use crate::radio_settle::SettleRadio;
    use crate::rewards::{
        claim_from_links, contributions_with_rho, settle_with_peer_accs, share_of, verify_receipt,
        TicketPolicy,
    };
    use crate::tickets::{easy_target, grind_settlement, self_fold};
    use tru::{Context as TruCtx, FocusingParams, Link};

    #[derive(Args, Debug)]
    pub struct SettleNetArgs {
        /// Data directory (secret key + state).
        #[arg(short, long, default_value = "~/.foculus-settle")]
        dir: String,
        /// QUIC bind port.
        #[arg(short, long, default_value = "4210")]
        port: u16,
        #[command(subcommand)]
        cmd: SettleNetCmd,
    }

    #[derive(Subcommand, Debug)]
    enum SettleNetCmd {
        /// Print this node's endpoint id + addr ticket and listen.
        Listen {
            /// Hex topic (64 chars).
            #[arg(long)]
            topic: Option<String>,
        },
        /// Multi-miner demo: grind SelfAcc, publish over radio, collect + settle.
        Demo {
            /// Peer JSON EndpointAddr (repeatable).
            #[arg(long = "peer")]
            peers: Vec<String>,
            /// Wait for this many peer SelfAccs before settle.
            #[arg(long, default_value = "0")]
            want: usize,
            /// Budget to allocate.
            #[arg(long, default_value = "1000")]
            budget: u64,
            /// Seconds to wait for peer accs.
            #[arg(long, default_value = "5")]
            timeout: u64,
        },
    }

    pub fn run(args: SettleNetArgs) -> Result<()> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        rt.block_on(async move { run_async(args).await })
    }

    async fn run_async(args: SettleNetArgs) -> Result<()> {
        let dir = expand_home(&args.dir);
        std::fs::create_dir_all(&dir)?;
        let radio = SettleRadio::start(&dir, args.port).await?;
        println!("settle-radio listening");
        println!("  id:   {}", radio.id_string());
        println!("  addr: {}", radio.addr_json());
        println!(
            "  alpn: {}",
            String::from_utf8_lossy(crate::radio_settle::SETTLE_ALPN)
        );

        match args.cmd {
            SettleNetCmd::Listen { topic } => {
                if let Some(t) = topic {
                    let topic = parse_topic(&t)?;
                    radio.subscribe(topic).await;
                    println!("  subscribed topic {}", to_hex(&topic));
                }
                println!("running… ctrl-c to stop");
                tokio::signal::ctrl_c().await?;
            }
            SettleNetCmd::Demo {
                peers,
                want,
                budget,
                timeout,
            } => {
                for p in &peers {
                    radio.add_peer_addr(parse_peer(p)?).await;
                }
                run_demo(&radio, want, budget, Duration::from_secs(timeout)).await?;
            }
        }
        radio.shutdown().await.ok();
        Ok(())
    }

    async fn run_demo(
        radio: &SettleRadio,
        want: usize,
        budget: u64,
        timeout: Duration,
    ) -> Result<()> {
        fn h(b: u8) -> [u8; 32] {
            let mut x = [0u8; 32];
            x[0] = b;
            x
        }
        let claims = vec![claim_from_links(
            h(0xA1),
            h(10),
            vec![Link::stake(h(2), h(1), 8000)],
            1,
        )];
        let ids: Vec<_> = claims.iter().map(|c| c.id).collect();
        let topic = claims_root(&ids);
        radio.subscribe(topic).await;
        println!("topic {}", to_hex(&topic));

        let base = vec![
            Link::stake(h(1), h(2), 100),
            Link::stake(h(2), h(3), 100),
            Link::stake(h(3), h(1), 100),
        ];
        let art = open_beacon(1, &GENESIS_PREV, &topic, &[], TEST_OUTER_T);
        let contribs = contributions_with_rho(&claims);
        let miner = h(0x91);
        let tickets = grind_settlement(
            &base,
            &contribs,
            &TruCtx::none(),
            &FocusingParams::default(),
            &art.beacon,
            &topic,
            &miner,
            0,
            64,
            4,
            easy_target(),
        );
        let acc = self_fold(contribs.len(), &tickets);
        let sent = radio
            .publish(SettleMsg::SelfAcc {
                topic,
                miner,
                acc: acc.clone(),
            })
            .await?;
        println!("published SelfAcc k={} to {sent} peer(s)", acc.k);

        let mut leaves = radio.wait_self_accs(&topic, want, timeout).await;
        println!("collected {} peer SelfAcc(s)", leaves.len());
        leaves.push(acc);

        let rec = settle_with_peer_accs(
            1,
            &GENESIS_PREV,
            &base,
            &claims,
            &TruCtx::none(),
            &FocusingParams::default(),
            budget,
            &TicketPolicy {
                want: 4,
                max_attempts: 64,
                miner,
                ..TicketPolicy::default()
            },
            &leaves,
        )
        .map_err(|e| anyhow::anyhow!("settle_with_peer_accs: {e:?}"))?;
        if !verify_receipt(&rec) {
            bail!("receipt verify failed");
        }
        println!("receipt {}", to_hex(&rec.receipt_hash));
        for s in &rec.shares {
            println!(
                "  neuron {}… amount {}",
                to_hex(&s.neuron[..4]),
                s.amount
            );
        }
        println!("share local={}", share_of(&rec, &h(10)));
        Ok(())
    }

    fn expand_home(s: &str) -> PathBuf {
        if let Some(rest) = s.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home).join(rest);
            }
        }
        PathBuf::from(s)
    }

    fn parse_topic(s: &str) -> Result<[u8; 32]> {
        let bytes = from_hex(s.trim()).context("topic hex")?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("topic must be 32 bytes hex"))?;
        Ok(arr)
    }

    fn parse_peer(s: &str) -> Result<EndpointAddr> {
        if let Ok(addr) = serde_json::from_str::<EndpointAddr>(s) {
            return Ok(addr);
        }
        let id: EndpointId = s
            .parse()
            .map_err(|e| anyhow::anyhow!("peer id/addr: {e}"))?;
        Ok(EndpointAddr::new(id))
    }

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn from_hex(s: &str) -> Result<Vec<u8>> {
        if s.len() % 2 != 0 {
            bail!("odd hex length");
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| anyhow::anyhow!(e.to_string()))
            })
            .collect()
    }
}

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
