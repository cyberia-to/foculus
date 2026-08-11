---
tags: docs, explanation, foculus, soft3, latency
crystal-type: process
crystal-domain: cyber
alias: latency targets, finality latency, money clocks, send receive latency, light client scope
---

# latency targets

architectural map of time in soft3: which clocks exist, who owns them, what cyb may promise for balance / send / receive / reward-after-link, and how the **light client** (headers + fold) sits inside that scope.

this document is explanation, not a parameters table. normative constants live in [[foculus parameters|parameters.md]]. the single-signal walkthrough with worked numbers lives in [[life of a signal]]. light-client join protocol lives in [[structural sync]] and [[cyber/light|light client]]. this page answers: *which latency is whose problem, what must work for money to be real, and how full node / cell / light client share one certainty model.*

---

## where this document lives (and why not tok / sigma)

| layer | owns | does not own |
|---|---|---|
| [[foculus]] | consensus finality, epoch schedule, beacon delay, settlement depth $d$, domain-local liveness; light path for tip after fold | coin conservation rules, UI labels |
| [[zheng]] | prove/verify cost; folding accumulator + decider (clock C machinery) | network propagation or $\phi^*$ threshold |
| [[tok]] / PLUMB | *what* a pay/mint/burn means, conservation, when an Intent is well-formed | how long until the network agrees the Intent is final |
| [[bbg]] | state after apply; Lens openings against a root | wall-clock convergence |
| cyb [[sigma]] | view of holdings on a Card / neuron | any protocol clock |
| cyb sense | delivery of NOTIFY after a verified credit | minting or finality |

latency targets for "when is money final / spendable / notifiable" are foculus clocks observed by tok state and cyb UI. they are not tok specs and not a sigma feature. if a number must be tuned, change foculus parameters; if a balance is wrong after finality, that is tok/bbg; if the UI lies about pending vs final, that is cyb.

soft3 only. no foreign chain schedule is in scope here.

---

## three node modes (all in scope)

money does not require replaying the whole graph. three modes share the same roots and the same finality rule; they differ in **what they store** and **how they obtain a trusted tip**.

```
full node              cell (cyb)                 light client
replay / hold          apply my                   headers + fold
all signals            namespaces                 decide(acc)
tri-kernel locally     local notes + sigma        openings only
size: unbounded        size: O(my slice)          size: ~constant
```

| mode | tip trust | balance / receive | send |
|---|---|---|---|
| full node | own apply of history | local state | prove against local state |
| cell | apply my namespaces + peer completeness | local notes + openings | prove with local secrets + witnesses |
| light client | checkpoint + `decide(folding_acc)` then fold each tip | Lens open against `BBG_root` | prove with local secrets; witnesses from peer openings |

**in scope:** the light client is a first-class path for cyb on thin devices — join via fold, hold tip root, open balances, verify receive, authorize send. it is not a deferred optimization.

**cell vs light:** a production cyb often *is* a cell that *embeds* the light-client tip path (fold + root) while still storing private notes and applying signals that touch the owner. pure light (openings only, no private apply) is the thinnest extreme; pure full node is the fattest. the product scope includes the thin path end-to-end.

normative join steps (structural-sync):

```
1. obtain checkpoint = (BBG_root, folding_acc, height)   ~232–240 B
2. final_proof = decide(folding_acc)                    zheng decider
3. verify(final_proof, BBG_root)                        ~10–50 μs
4. open namespaces of interest                          Lens ~200 B each
5. maintain: fold each new block; refresh openings
```

without fold, a thin client only has social checkpoints. **with fold in scope, tip trust is math.** that is the difference the scope buys.

---

## three clocks (do not mix them)

money UX collapses if three different times are sold as one number.

### clock A — transfer finality

when a pay cyberlink / PLUMB Intent is canonical: nullifier committed, outputs spendable, conflicts pruned.

- owner: foculus ($\phi^*_i > \tau$ inside the $\varepsilon$-support domain, with completeness)
- unit of work: one signal (or atomic multi-pay Intent)
- UX: "sent", "received", "balance spendable for respend"
- independent of: epoch, Shapley
- tip context: whoever evaluates A (full node, cell, or light) does so against a **trusted tip root** — light obtains that root via clock C

### clock B — attribution settlement

when a reward for contribution (settle / mint pulse for $\Delta\phi^+$) is computed, escrowed, then spendable after reorg depth $d$.

- owner: foculus epoch pipeline (propose → beacon → settle → fold → tok mint)
- unit of work: epoch cluster + $d$ epochs
- UX: "you earned X for link L" when the payee set includes you
- independent of: whether a pure pay (no attribution) already finalized on clock A

a pure send/receive never waits on clock B. a link that only stakes conviction may still credit settle rewards on clock B. an Intent can do both: pay legs finalize on A; settle share on B.

### clock C — history trust (light client) — in scope

when a thin client is sure the tip root continues valid history from genesis (or from an epoch checkpoint that itself is fold-backed).

- owner: [[zheng]] folding accumulator + structural-sync light path + foculus headers
- unit of work: join once (`decide`), then O(1) fold per block
- UX: "empty disk → trusted tip"; "balance/receive proofs bind to that tip"
- role in money: **provides the root that openings and finality claims are checked against** on light/cell thin path

clock C is how the light client is *sure the tip is the chain*. clock A is how it is *sure this pay is final at that tip*. both are in scope for send/receive on thin cyb.

---

## architectural stack (money path, light included)

```
cyb sense  ── observes events ──► NOTIFY(payee), optional payer echo
cyb sigma  ── observes balances ► openings or local apply

        ▲ Lens openings / local apply events
        │
   ┌────┴────────────────────────────────────┐
   │  tip root (BBG_root)                      │
   │    full: own apply                        │
   │    cell: apply my slice + completeness    │
   │    light: decide(acc) + fold headers  ◄── clock C (in scope)
   └────┬────────────────────────────────────┘
        │ finality of pays that touch me
        │
   foculus     clock A: φ* > τ (+ completeness proofs for domain)
               clock B: epoch settle + depth d
        ▲
        │ valid signals only
        │
   zheng σ     prove Intent / cyberlink (send path)
   tok PLUMB   conservation of pays in the Intent
   bbg         state committed at BBG_root
```

### what must work for send/receive (updated scope)

| # | requirement | full | cell | light |
|---|---|---|---|---|
| 1 | zheng validity of spends | yes | yes | yes (verify peer/self proofs) |
| 2 | foculus finality clock A | yes | yes | yes (via certified domain view / openings against tip) |
| 3 | local secrets for my notes | yes | yes | yes |
| 4 | trusted tip root | own history | sync + completeness | **fold + decide (clock C)** |
| 5 | balance / receive visibility | local state | apply + open | **Lens open against tip** |
| 6 | sense NOTIFY | from apply | from apply / open | from open + event subscription |

clock B remains required only when the product shows attribution rewards (still in product scope for reward-after-link).

### how a light client is sure it sent / received

send:

1. hold tip via clock C (fold maintained)
2. obtain witnesses (membership / note openings) against `BBG_root`
3. prove pay Intent (zheng + PLUMB)
4. gossip; peers reject invalid $\sigma$
5. observe finality of the spend particle at tip (clock A) — completeness / nullifier commitment as proved relative to tip
6. UI grade 2 only after A at that tip

receive:

1. same tip (clock C)
2. peer (or push channel) offers credit + Lens opening of my balance/note or output
3. verify opening against tip `BBG_root`
4. when the creating signal is final at tip (clock A), mark received and enable respend once the new note is held
5. sense NOTIFY on verified credit, not on unauthenticated push alone

so: light client in scope means **money certainty = C (tip) + A (this pay) + openings**, not "trust the peer's JSON".

---

## latency targets

numbers are Earth-scale, hub-domain typical. sparse domains and near-50/50 conflicts are slower by construction (see [[foculus security at scale]] S5, T1). conservative vs tuned matches [[life of a signal]].

### clock A — transfer (send / receive / respend)

| stage | typical | notes |
|---|---|---|
| prove $\sigma$ (local) | ms–tens of ms class | circuit-dependent |
| gossip across domain | ~0.4–3 s | $O(\Delta\log\|D\|)$; invalid dies at verify |
| $\phi^*$ contraction to $\tau$ | ~1–4 s hub | tens of s sparse fringe; longer under support-switching |
| final + outputs spendable | ~1–4 s typical | deterministic; no confirmation-depth policy |
| light: open balance after final | μs–ms verify + RTT | opening ~200 B against tip |
| receiver sense NOTIFY | after verified credit at tip | never on bare unauthenticated push |
| sigma refresh | after apply or verified open | |

acceptance for cyb pure pay: mark received / allow respend only at clock A final against a clock-C (or full/cell-equivalent) tip — never on "submitted".

### clock B — reward after link (attribution)

| stage | conservative | tuned | notes |
|---|---|---|---|
| link final (clock A) | ~4 s | ~1–4 s | knowledge / stake settled |
| epoch total | ~85 s | ~25 s | propose + beacon + settle + fold + mint |
| spendable at depth $d=2$ | ~3 min | ~50 s | escrow until reorg depth |
| floor (mature, short epoch) | — | ~40 s class | physics + stats bound |

epoch length is a latency UX knob, not a security parameter once $k_{\min}$ and beacon/VDF bounds hold. light client observes settle mints the same way as any other credit: opening against tip after mint is committed.

### clock C — light client join and tip follow (in scope)

| stage | target | notes |
|---|---|---|
| download checkpoint + acc | ~200–few KB | from any peer; untrusted until decide |
| `decide(folding_acc)` verify | ~10–50 μs class | proves history to that height |
| open my namespaces | O(owned) + ~200 B / open | balances, notes, pending outs |
| steady-state per tip | O(1) fold (~30 field ops class) + header | no re-decide from genesis |
| re-join after long offline | one decide on latest acc | same as first join |

join latency is dominated by **download + RTT**, not by verify. verify is effectively free compared to network. product scope: cold start on phone must complete fold verify before treating openings as money-grade.

---

## multi-payee rewards (both receivers)

one Intent may credit several payees. latency is per credit leg, not per "mode".

| leg | clock | who gets sense |
|---|---|---|
| pay neuron → neuron (or card) | A at tip | payee (TransferIn); optional payer outbox |
| pay / stake that settles as attribution mint | B then spendability after $d$ | each payee of the settle share |
| linker self-reward + owner tip in one Intent | A for tip; B for settle share if any | both neurons |

cyb does not pick a single policy. sigma updates every balance that moved; sense notifies every payee (and may echo the payer). light client verifies each credit opening independently against the same tip.

---

## certainty grades (what "sure" means)

| grade | meaning | UI |
|---|---|---|
| 0 local author | I signed and emitted | composing / submitted locally |
| 1 network-valid | peers accept $\sigma$; still conflict-capable | pending |
| 2 final (clock A) at trusted tip | $\phi^* > \tau$, nullifier in $N$, outputs spendable | sent / received / spendable |
| 3 settled reward (clock B) | mint/escrow after epoch + depth $d$ | earned, then spendable |
| 4 history-audited tip (clock C) | `decide(acc)` (or full equivalent) holds for current tip | device joined; openings money-grade |

for **light cyb**, grade 2 without grade 4 is incomplete: finality claims need a tip you trust. scope rule:

- full node: grade 4 equivalent via own history; then grade 2 for each pay  
- cell: grade 4 via continuous completeness + sync (and fold when available); grade 2 for pays  
- light client: **grade 4 first (fold), then grade 2 per pay via openings**

send/receive on light = grade 4 ∧ grade 2. reward-after-link settle = grade 3 on top when applicable.

---

## what cyb may promise (scope checklist)

| promise | clocks | in scope | must work |
|---|---|---|---|
| show balance | tip + open/apply | yes | bbg Lens or local; tip from C or full/cell |
| send coin | A (+ C on light) | yes | prove, gossip, finalize, verify at tip |
| receive coin + notification | A + sense (+ C on light) | yes | verified opening + NOTIFY |
| reward after link (pay legs) | A | yes | multi-payee Intent |
| reward after link (attribution) | B | yes | epoch pipeline + openings of mint |
| cold-start trust history | C | **yes** | folding accumulator + decider |
| fold tip every block | C steady-state | **yes** | O(1) fold maintain |

do not advertise "instant finality" for settle rewards. do not treat unauthenticated peer balance as received. do not ship light money UX without clock C.

---

## comparison anchors (Earth)

| system class | transfer finality | thin-client history | attribution |
|---|---|---|---|
| classic BFT | ~5–60 s | light client / 2/3 set | usually none |
| nakamoto-style | tens of min probabilistic | SPV / headers | luck / fees |
| foculus + tok + zheng | clock A ~1–4 s typical | clock C one decide + fold | clock B ~1–3 min / tuned sub-minute |

---

## open tensions (named, not hidden)

- domain variance: hub ~seconds, fringe tens of seconds — one global SLA is a lie  
- near-symmetric conflict: support-switching adds time (T1)  
- epoch vs transfer: UI must separate "paid" from "earned for linking"  
- light finality observation: pure light does not run tri-kernel — it consumes finality/completeness proofs and openings bound to tip; the proof surface for "this particle is final" must be explicit in specs (provable consensus / VEC)  
- witness availability: send on light needs note witnesses from peers; DAS/completeness must not stall honest spends  
- cross-domain / interplanetary: light-delay bounds — see [[interplanetary]]

---

## see also

- [[specs/money-loop]] — normative network money product (cyber/specs)  
- [[specs/light-money]] — light tip + send/receive implementation contract  
- [[specs/node-modes]] — full / cell / light duties  
- [[specs/component-ownership]] — who implements which work package  
- [[life of a signal]] — second-by-second walkthrough and conservative/tuned epoch table  
- [[foculus overview]] — finality as $\phi^* > \tau$  
- [[foculus protocol]] — normative finalize rule, nullifiers, performance table  
- [[foculus parameters]] — where knobs are fixed  
- [[structural sync]] — five layers; light client join protocol  
- [[cyber/light|light client]] — header spine and query openings  
- [[provable consensus]] — proving $\phi^*$ for thin verifiers  
- [[tok]] / PLUMB — pay, mint, Intent atomicity (conservation, not clocks)  
- [[zheng]] recursion / accumulator — clock C machinery  
- reward specification / fold-mining — clock B machinery  

---

discover all [[concepts]]
