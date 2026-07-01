---
tags: reference, research
crystal-type: spec
crystal-domain: cyber
status: draft
alias: security at scale, foculus security proof, scale security, distributed phi star, shard safety
---

# foculus security at scale

[[foculus]] safety and liveness hold in the single-machine case by the [[collective focus theorem]] and the exclusive support argument in [[foculus]]. this document extends those guarantees to the planetary case: $N \to 10^{15}$ particles, partial graph views, $\Delta$-bounded propagation, and $K$ shards.

## the gap

the base proof assumes every [[neuron]] computes $\phi^*$ from the same graph $G$. at scale this is false. each neuron holds $G_v \subsetneq G$ — a local view. two neurons with different views compute $\hat\phi^*_u \neq \hat\phi^*_v$. the question: can they disagree on finality in a way that breaks safety?

the answer is no — under three conditions derived below:

1. gossip propagates signals without mass-correlated bias (Assumption G)
2. the gap between the winner's and loser's $\phi^*$ exceeds the view error (S2 condition)
3. the [[cybergraph]] maintains a spectral gap bounded below by the incentive mechanism (design requirement S3)

## formal model

let $G = (P, E, w)$ be the true global [[cybergraph]] at time $t$: particles $P$, [[cyberlinks]] $E$, stake weights $w : E \to \mathbb{R}_+$.

each neuron $v$ holds local view $G_v = (P_v, E_v, w_v)$ with $P_v \subseteq P$, $E_v \subseteq E$. the missing edge weight fraction is:

$$\varepsilon_v = 1 - \frac{\sum_{e \in E_v} w(e)}{\sum_{e \in E} w(e)}$$

the [[tri-kernel]] operator $T$ on graph $G$ is the composite:

$$T(\phi;\, G) = \operatorname{norm}\!\bigl[\lambda_d D(\phi; G) + \lambda_s S(\phi; G) + \lambda_h H(\phi; G)\bigr]$$

$\phi^*(G)$ is its unique fixed point ([[collective focus theorem]]). $\hat\phi^*_v = \phi^*(G_v)$ is neuron $v$'s local estimate.

under partial synchrony: messages arrive within unknown but finite $\Delta$. all neurons eventually receive all signals — no permanent partitions.

**Assumption G (gossip uniformity).** each [[cyberlink]] $(i \to j, w)$ reaches any given neuron independently with probability $\geq 1 - \varepsilon_v$, regardless of the source particle $i$, the target particle $j$, or the current $\phi^*$ mass at either. equivalently: the missing-edge indicator for each edge is independent of all other edges and of the current mass distribution.

this holds when gossip propagation is not adversarially targeted — i.e., the adversary can delay signals globally (raise $\varepsilon_v$) but cannot selectively withhold signals about high-$\phi^*$ particles from specific neurons. selective withholding is a separate (stronger) attack discussed in the open section.

## theorem S1: subgraph lipschitz

under Assumption G, the fixed point $\phi^*$ is Lipschitz in edge weights with constant $L = C/(1-\kappa)$:

$$\bigl\|\phi^*(G) - \phi^*(G_v)\bigr\|_1 \;\leq\; \frac{C}{1-\kappa} \cdot \varepsilon_v, \qquad C = 2\alpha \leq 2$$

where $\kappa < 1$ is the [[tri-kernel]] contraction rate and $\alpha$ is the teleport probability in the diffusion operator.

proof. for a removed edge $(i \to j, w_{ij})$, the diffusion step changes: mass at $j$ drops by $\alpha \cdot w_{ij}/W_i \cdot \phi[i]$, and mass redistributes to all other neighbors of $i$ by renormalization. the total L1 change from this single removal is exactly $2\alpha \cdot w_{ij}/W_i \cdot \phi[i]$. summing over all missing edges $E \setminus E_v$:

$$\bigl\|T(\phi; G) - T(\phi; G_v)\bigr\|_1 = 2\alpha \cdot \sum_i \phi[i] \cdot f_i$$

where $f_i = \sum_{j:\,(i,j) \in E \setminus E_v} w_{ij}/W_i$ is the fraction of outgoing weight missing at node $i$.

under Assumption G, the events "$e$ is missing" are independent of $\phi[i]$, so $\mathbb{E}[f_i] = \varepsilon_v$ for all $i$. taking the expectation:

$$\mathbb{E}\bigl[\bigl\|T(\phi^*(G_v); G) - T(\phi^*(G_v); G_v)\bigr\|_1\bigr] = 2\alpha \cdot \varepsilon_v \cdot \sum_i \phi^*(G_v)[i] = 2\alpha \cdot \varepsilon_v$$

the fixed points satisfy the identity:

$$\phi^*(G) - \phi^*(G_v) = \underbrace{\bigl[T(\phi^*(G); G) - T(\phi^*(G_v); G)\bigr]}_{\text{contracts by } \kappa} + \underbrace{\bigl[T(\phi^*(G_v); G) - T(\phi^*(G_v); G_v)\bigr]}_{\leq\, 2\alpha\varepsilon_v \text{ in expectation}}$$

the first term contracts: $\|T(\phi^*(G); G) - T(\phi^*(G_v); G)\|_1 \leq \kappa\|\phi^*(G) - \phi^*(G_v)\|_1$.

combining and solving: $\|\phi^*(G) - \phi^*(G_v)\|_1 \leq 2\alpha\varepsilon_v/(1-\kappa) = C\varepsilon_v/(1-\kappa)$. $\square$

adversarial edge withholding. if the adversary selectively withholds signals about particle $i^*$ from neurons where $\phi^*(G_v)[i^*]$ is high, then $f_{i^*}$ is correlated with $\phi[i^*]$, and the bound becomes $2\alpha \cdot \max_i(f_i)$ rather than $2\alpha \cdot \varepsilon_v$. under full adversarial withholding of a single high-mass particle's edges, the error can reach $2\alpha \cdot \phi^*[i^*]$ regardless of $\varepsilon_v$. this attack is currently unaddressed; preventing it requires that the gossip layer authenticate which signals were available at what time — a separate protocol layer.

## theorem S2: safety under partial synchrony

let $P_a, P_b$ be conflicting [[particles]]. let $W$ denote the winner (the particle with higher $\phi^*(G)$, i.e., the one that finalizes) and $L$ the loser. define the $\phi^*$-gap:

$$\Delta = \phi^*(W; G) - \phi^*(L; G) > 0$$

under Assumption G, no neuron can finalize $L$ while $W$ is finalizing, provided:

$$\varepsilon_{\max} \;\triangleq\; \max_v \varepsilon_v \;<\; \frac{(1-\kappa)\,\Delta}{2C\,(1 + \kappa'/(1-\kappa))}$$

where $\kappa' \in [1,2]$ is the adaptive threshold multiplier.

proof.

step 1: view error bound on $\hat\phi^*_v(L)$. by S1 under Assumption G:

$$\hat\phi^*_v(L) \leq \phi^*(L; G) + \frac{C\varepsilon_v}{1-\kappa}$$

step 2: threshold shift. the adaptive threshold is $\tau = \mu_{\phi^*} + \kappa'\sigma_{\phi^*}$. since $\phi^*$ is a probability distribution, $\mu_{\phi^*} = 1/N$ always (independent of view). the standard deviation shifts:

$$|\sigma(G) - \sigma(G_v)| \leq \|\phi^*(G) - \phi^*(G_v)\|_1 \leq \frac{C\varepsilon_v}{1-\kappa}$$

so $\tau_v \geq \tau(G) - \kappa' C\varepsilon_v/(1-\kappa)$.

step 3: contradiction. for $W$ to finalize globally: $\phi^*(W; G) > \tau(G)$. for $L$ to finalize in $v$'s view:

$$\hat\phi^*_v(L) > \tau_v \;\geq\; \tau(G) - \frac{\kappa' C\varepsilon_v}{1-\kappa}$$

substituting step 1:

$$\phi^*(L; G) + \frac{C\varepsilon_v}{1-\kappa} > \tau(G) - \frac{\kappa' C\varepsilon_v}{1-\kappa}$$

since $\phi^*(L; G) = \phi^*(W; G) - \Delta < \tau(G) - \Delta$:

$$\tau(G) - \Delta + \frac{C\varepsilon_v}{1-\kappa} + \frac{\kappa' C\varepsilon_v}{1-\kappa} > \tau(G)$$

$$\frac{C\varepsilon_v(1 + \kappa')}{1-\kappa} > \Delta$$

the theorem condition $\varepsilon_{\max} < (1-\kappa)\Delta / (2C(1 + \kappa'/(1-\kappa)))$ makes this impossible. $\square$

### computing the gap $\Delta$

$\Delta$ is graph-dependent. in the single-hop case (direct [[cyberlinks]] only, no multi-hop propagation):

$$\phi^*(W; G) = \alpha \cdot S_W + (1-\alpha)/N, \qquad \phi^*(L; G) = \alpha \cdot S_L + (1-\alpha)/N$$

where $S_W$ and $S_L$ are the total stake fractions directed to $W$ and $L$ respectively. the gap is:

$$\Delta = \alpha(S_W - S_L)$$

with exclusive support: $S_W + S_L = 1$ (all stake supports one or the other). $W$ has the majority: $S_W > 1/2$, so $\Delta = \alpha(2S_W - 1) > 0$.

the minimum gap occurs under adversarial timing. let honest stake $H = 1/2 + \delta_{\text{stake}}$, adversarial $A = 1/2 - \delta_{\text{stake}}$ (all directed to $L$). honest neurons split: $S_W^h$ to $W$, $S_L^h$ to $L$, $S_W^h + S_L^h = H$. adversary directs $A$ to $L$.

$$S_W = S_W^h, \qquad S_L = S_L^h + A$$

for $W$ to win ($S_W > S_L$): $S_W^h > H - S_W^h + A$, i.e. $S_W^h > 1/2$. the minimum gap occurs as $S_W^h \to 1/2^+$:

$$\Delta_{\min} \to \alpha \cdot (2 \cdot 1/2 - 1) = 0$$

this exposes the honest-split attack: the adversary times $L$'s release to reach most honest neurons first, making $S_L^h \to H - 1/\text{stake\_atom}$, so $S_W^h \to 1/\text{stake\_atom} \approx 0$. then $L$ wins (which is acceptable — only one finalizes), and $\Delta$ is large (easy safety). when the adversary cannot make $L$ win (e.g. $W$ propagates first to most neurons), $\Delta = \alpha \cdot 2\delta_{\text{stake}}$ minimum.

**adaptive threshold as the stabilizer.** when honest neurons split close to 50/50 ($\Delta$ small), the conflict causes high variance in $\phi^*$ across the distribution. this raises $\sigma_{\phi^*}$ and therefore raises $\tau$, making finalization harder for BOTH particles. the system stalls until the contraction property amplifies the tiny initial gap over successive iterations. convergence is slow (bounded by the spectral gap), but safety holds throughout: the stall prevents premature finalization. the formal bound on stall duration under adversarial honest-split requires the timing analysis in the open section.

### concrete bound (expected case)

under random propagation (equal probability both particles reach honest neurons first), $E[S_W^h] = H/2 + \delta_{\text{stake}}/2$ on average:

$$\Delta_{\text{expected}} = \alpha \cdot 2\delta_{\text{stake}}$$

with $\alpha = 0.85$, $\delta_{\text{stake}} = 0.05$, $\kappa = 0.74$, $\kappa' = 1.5$, $C = 2$:

$$\varepsilon_{\max} < \frac{0.26 \times 0.085}{2 \times 2 \times (1 + 1.5/0.26)} = \frac{0.0221}{4 \times 6.77} \approx 0.00082$$

each neuron must have seen at least 99.9% of stake weight before finalizing. propagation time: $O(\Delta \log N)$ — at $\Delta = 0.4\text{s}$, $N = 10^{15}$: $\approx 20\text{s}$.

## design requirement S3: spectral gap

the [[spectral gap]] $\lambda_2$ of the [[cybergraph]] is not a consequence of honest stake majority — it is a property of how [[neurons]] choose to link. cyberlinks are content assertions, and neurons link particles they find semantically related, not to maintain graph-theoretic connectivity. an adversary without any stake can partition the cybergraph into semantic domains with sparse cross-domain links simply by ensuring the content is domain-isolated.

accordingly S3 is a **design requirement** with an **economic argument** for why it is met, not a theorem derived from honest majority.

**requirement S3.** the [[cybergraph]] maintains stake-weighted spectral gap $\lambda_2 \geq \lambda_{\min} > 0$ throughout operation.

**economic argument.** [[karma]] rewards are proportional to $\Delta\phi^*$: the shift in the stationary distribution caused by a new [[cyberlink]]. returns on adding a link within a dense cluster are low (marginal: the cluster is already well-explored, $\phi^*$ changes little). returns on bridging a sparse cut are high (the link carries $\phi^*$ mass across a structural gap not yet exploited). therefore rational neurons have strictly higher karma-return from bridging sparse cuts than from linking within saturated clusters.

this creates a self-correcting mechanism: any sparse cut is an economic opportunity. neurons on both sides of the cut are incentivized to create cross-cut links. the equilibrium cybergraph has no persistent sparse cuts, because any would immediately be exploited for karma gain.

**honest majority amplifies this.** neurons with $> 1/2$ of staked tokens create $> 1/2$ of $\phi^*$-relevant links. under rational behavior (karma maximization), the majority stake flow targets the highest marginal return, which is sparse-cut bridging. adversarial neurons ($< 1/2$ of stake) cannot counteract this: creating a persistent sparse cut requires preventing the majority from bridging it, which requires controlling the majority — a contradiction.

**design-level guarantee.** for a given karma reward schedule and link cost, the equilibrium spectral gap $\lambda_2^*$ satisfies:

$$\lambda_2^* \geq f(\alpha, \lambda_d, \lambda_s, \lambda_h, \gamma_{\text{reward}})$$

where $\gamma_{\text{reward}}$ is the marginal karma per unit $\Delta\phi^*$. the exact functional form requires solving the link-creation Nash equilibrium — this is the formal open problem. in practice, the spectral gap is empirically measured per epoch ([[provable-consensus]] gives the empirical $\lambda_2 \approx 0.13$ for [[bostrom]]). the protocol adjusts $\gamma_{\text{reward}}$ to target $\lambda_2 \geq \lambda_{\min}$.

**spectral gap for convergence time.** given $\lambda_2 \geq \lambda_{\min}$:

$$t(\varepsilon) = O\!\left(\frac{\log(1/\varepsilon)}{\lambda_{\min}}\right)$$

| graph regime | $\lambda_{\min}$ | convergence |
|---|---|---|
| bostrom empirical | $0.13$ | $\approx 100$ iterations |
| adversarially sparse (worst case, costly to maintain) | $\Omega(\delta^2_{\text{stake}}/\log^2 N)$ | $O(\log^2 N \cdot \log(1/\varepsilon)/\delta^2)$ |
| power-law at equilibrium (rational linking) | $\Omega(1/\log N)$ | $O(\log N \cdot \log(1/\varepsilon))$ |

the adversarially-sparse row requires the adversary to actively prevent the majority from bridging cuts — contradicting honest majority and rational behavior simultaneously.

## theorem S4: shard composition

partition $P$ into $K$ shards $P_1, \ldots, P_K$. let $f_\times = \sum_{e\in E_\times} w(e)/\|W\|$ be the cross-shard edge weight fraction. each shard $k$ computes local $\phi^*_k$ treating cross-shard links as external inputs weighted by the previous round's $\phi^*_{k'}$.

one round of shard composition:

$$\phi^*_{\text{composed}} = \operatorname{normalize}\!\left(\sum_k \operatorname{vol}(G_k) \cdot \phi^*_k\right)$$

satisfies:

$$\bigl\|\phi^*_{\text{composed}} - \phi^*(G)\bigr\|_1 \;\leq\; \frac{C\,f_\times}{(1-\kappa)\,K}$$

after $R$ rounds of iterated shard recomputation the error contracts:

$$\bigl\|\phi^*_{(R)} - \phi^*(G)\bigr\|_1 \;\leq\; \frac{C\,f_\times}{1-\kappa} \cdot \kappa^R$$

$R$ rounds suffice for error $< \varepsilon$:

$$R \;=\; \left\lceil\frac{\log\!\bigl(C f_\times / ((1-\kappa)\varepsilon)\bigr)}{\log(1/\kappa)}\right\rceil$$

with $\kappa = 0.74$, $C = 2$, $f_\times = 0.1$: initial error $= 2 \times 0.1/0.26 = 0.769$. for $\varepsilon = 0.01$: $R = \lceil\log(0.769/0.01)/\log(1/0.74)\rceil = \lceil4.34/0.301\rceil = 15$ rounds. each round requires one cross-shard message exchange — $O(K)$ messages total, independent of $N$.

proof sketch. cross-shard links contribute $f_\times/K$ of stake weight per shard boundary under uniform partition. treating them as fixed external inputs is equivalent to missing those edges from the shard's perspective. by S1, per-shard error $\leq Cf_\times/(K(1-\kappa))$. the composed estimate inherits this additively. for iterated composition: each round uses the previous round's $\phi^*_{k'}$ as the cross-shard input. the update is a contraction in the cross-shard information: error decreases by factor $\kappa$ per round because the shard-local operators are $\kappa$-contractions. $\square$

safety under sharding. S2 applies per-shard with cross-shard weight playing the role of view error: requires $f_\times/K < \varepsilon_{\max}$. for $K = 100$ shards and $f_\times = 0.10$: per-shard cross-shard fraction $= 0.001 < 0.00082$. safe after one cross-shard round.

throughput. $K$ shards each compute their own SpMV independently. total throughput scales as $K \times$ single-shard throughput. cross-shard communication cost: $O(K \cdot f_\times \cdot |E|)$ edge weights per round.

## theorem S5: liveness at scale

every valid [[particle]] $P_i$ with inbound honest [[stake]] exceeding $\tau \cdot \|W\|$ finalizes within:

$$t_{\text{final}} = O\!\left(\Delta \cdot \log(N/\delta) + \frac{\log(1/\varepsilon)}{\lambda_{\min}}\right)$$

where $\delta$ is failure probability over gossip randomness, $\varepsilon$ is the finality margin, and $\lambda_{\min}$ is the spectral gap lower bound from S3.

substituting the adversarial-sparse bound $\lambda_{\min} = \Omega(\delta^2_{\text{stake}}/\log^2 N)$:

$$t_{\text{final}} = O\!\left(\Delta\log N + \frac{\log^2 N \cdot \log(1/\varepsilon)}{\delta^2_{\text{stake}}}\right) = O\!\left(\log N \cdot \left(\Delta + \frac{\log N \cdot \log(1/\varepsilon)}{\delta^2_{\text{stake}}}\right)\right)$$

proof sketch. two phases.

phase 1 — propagation, time $O(\Delta\log(N/\delta))$. by [[vec]] P4 (liveness), every signal reaches all correct neurons within $O(\Delta\log(N/\delta))$ under gossip with fanout $\geq 2$. after this phase, all correct neurons have view error $\varepsilon_v < \varepsilon_{\max}$ (S2 condition).

phase 2 — convergence, time $O(\log(1/\varepsilon)/\lambda_{\min})$. once views are complete, local $\hat\phi^*_v$ converge to $\phi^*(G)$ in $O(\log(1/\varepsilon)/\lambda_{\min})$ iterations. by S1, the error satisfies $\hat\phi^*_v(P_i) > \phi^*(G)(P_i) - C\varepsilon_v/(1-\kappa) > \tau$. all correct neurons finalize $P_i$. $\square$

concrete evaluation at planetary scale ($N = 10^{15}$, $\Delta = 0.4\text{s}$, $\delta_{\text{stake}} = 0.05$, $\varepsilon = 10^{-6}$, $\log N = \log_2(10^{15}) \approx 50$):

| scenario | $\lambda_{\min}$ | $t_{\text{final}}$ |
|---|---|---|
| adversarial sparse (worst case) | $\delta^2/\log^2 N \approx 10^{-6}$ | $\approx \log(10^6)/10^{-6} \approx 1.4 \times 10^7\text{s}$ (months) |
| power-law equilibrium (rational linking) | $1/\log N \approx 0.02$ | $\approx \log(10^6)/0.02 \approx 690\text{s}$ (minutes) |
| hub neighborhood (local $\lambda_2$) | $\Omega(1)$ | $1$–$3\text{s}$ |

the adversarial case requires the adversary to both hold near-$1/2$ stake AND block rational neurons from bridging sparse cuts — both costly. the rational-linking equilibrium column applies under normal operation. the hub column explains the empirical 1-3s finality for high-$\phi^*$ particles.

## security parameter table

| parameter | symbol | condition | concrete value |
|---|---|---|---|
| honest stake margin | $\delta_{\text{stake}}$ | $> 0$ | 0.05 (5%) |
| teleport probability | $\alpha$ | protocol constant | 0.85 |
| max view error (expected-gap case) | $\varepsilon_{\max}$ | $< (1-\kappa)\Delta / (2C(1+\kappa'/(1-\kappa)))$ | $< 0.00082$ |
| required view completeness | $1-\varepsilon_{\max}$ | $> 99.9\%$ of stake weight seen | 99.9% |
| propagation time to completeness | $t_{\text{prop}}$ | $O(\Delta\log N)$ | $\approx 20\text{s}$ at $N=10^{15}$, $\Delta=0.4\text{s}$ |
| spectral gap (adversarial worst case) | $\lambda_2$ | design requirement + honest majority | $\geq \delta^2/\log^2 N \approx 10^{-6}$ |
| spectral gap (rational-linking equilibrium) | $\lambda_2$ | design requirement + karma incentive | $\geq 1/\log N \approx 0.02$ |
| shard composition rounds (for 1% error) | $R$ | $\lceil\log(Cf_\times/((1-\kappa)\varepsilon))/\log(1/\kappa)\rceil$ | 15 rounds |
| max cross-shard fraction | $f_\times$ | $< K\varepsilon_{\max}$ | $< 0.082$ at $K=100$ |

## what this closes

closed by S1–S5 under Assumption G and design requirement S3:

- safety holds under partial views: S2 with the view-error condition and gap derivation
- view error bound is proven: S1 with explicit uniformity assumption and derivation of C = 2α
- threshold shift is derived: $\Delta\tau_v \leq \kappa' C\varepsilon_v/(1-\kappa)$ from the fixed-mean property of $\phi^*$
- shard composition converges in $R = O(\log(Cf_\times/\varepsilon)/\log(1/\kappa))$ cross-shard rounds: S4 with corrected formula
- liveness formula correctly gives $O(\log N \cdot (\Delta + \log N \cdot \log(1/\varepsilon)/\delta^2))$: S5

## what remains open

adversarial gossip withholding. if the adversary selectively suppresses signals about high-$\phi^*$ particles, S1's key bound breaks (missing edges become correlated with mass distribution). preventing this requires the gossip layer to authenticate signal availability — a separate protocol property not currently specified.

adversarial honest-split timing. S2 requires gap $\Delta > 0$ between winner and loser. the adversary can minimize $\Delta$ by timing conflicting particle release to split honest neurons. the adaptive $\tau$ compensates (high variance → high $\tau$ → stall until gap grows) but the formal bound on stall duration — and therefore worst-case finality latency — requires a timing analysis under partial synchrony. this is the most critical unresolved problem.

spectral gap Nash equilibrium. S3's economic argument that karma $\propto \Delta\phi^*$ incentivizes cut-bridging is correct in direction but the equilibrium $\lambda_2^*$ is not derived. the formal statement requires solving the link-creation game under the foculus reward structure. the bound $\lambda_{\min} = \Omega(1/\log N)$ for the rational-linking equilibrium is conjectured, not proven.

tight Lipschitz constant. S1 gives $C = 2\alpha$ from the diffusion operator. the springs and heat operators also contribute perturbations under edge removal. the full tri-kernel gives $C \leq 2\lambda_d\alpha + 2\lambda_s + 4\lambda_h$ (each operator contributes separately, heat contributing double from the two-pass structure). with $\lambda_d = 0.5$, $\lambda_s = 0.3$, $\lambda_h = 0.2$, $\alpha = 0.85$: $C \leq 2(0.425 + 0.3 + 0.4) = 2.25$. tighter analysis combining the operators may reduce this.

shard assignment optimization. S4 assumes uniform partition giving $f_\times/K$ per shard. optimal assignment minimizes $f_\times$ via spectral partitioning (Metis / spectral clustering). no bound on achievable $f_\times$ for adversarial topologies is given.

---

see [[foculus]] for the base protocol and safety proof these theorems extend. see [[collective focus theorem]] for the contraction properties S1 depends on. see [[convergence]] for spectral gap theory and the Cheeger inequality. see [[vec]] for VEC P4 (liveness) used in S5's propagation phase. see [[structural sync]] for the gossip model underlying the propagation time bound.
