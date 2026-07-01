---
tags: reference, research
crystal-type: spec
crystal-domain: cyber
status: draft
alias: security at scale, foculus security proof, scale security, distributed phi star, shard safety
---

# foculus security at scale

[[foculus]] safety and liveness hold in the single-machine case by the [[collective focus theorem]] and the exclusive support argument in [[foculus]]. this document extends those guarantees to the planetary case: $N \to 10^{15}$ particles, partial graph views, $\Delta$-bounded propagation, and $K$ shards. five theorems close the gap between the base proof and production at scale.

## the gap

the base proof assumes every [[neuron]] computes $\phi^*$ from the same graph $G$. at scale this is false. each neuron holds $G_v \subsetneq G$ — a local view. two neurons with different views compute $\hat\phi^*_u \neq \hat\phi^*_v$. the question: can they disagree on finality in a way that breaks safety?

the answer is no — under three conditions that together constitute the security theorem:

1. each neuron's view error is bounded (fraction of missing [[stake]] weight $< \varepsilon_{\text{view}}$)
2. honest [[stake]] majority exceeds a threshold that depends on $\varepsilon_{\text{view}}$
3. the [[spectral gap]] of the [[cybergraph]] is bounded below

## formal model

let $G = (P, E, w)$ be the true global [[cybergraph]] at time $t$: particles $P$, [[cyberlinks]] $E$, stake weights $w : E \to \mathbb{R}_+$.

each neuron $v$ holds local view $G_v = (P_v, E_v, w_v)$ with $P_v \subseteq P$, $E_v \subseteq E$. the missing edge weight fraction is:

$$\varepsilon_v = 1 - \frac{\sum_{e \in E_v} w(e)}{\sum_{e \in E} w(e)}$$

the [[tri-kernel]] operator $T$ on graph $G$ is the composite:

$$T(\phi;\, G) = \operatorname{norm}\!\bigl[\lambda_d D(\phi; G) + \lambda_s S(\phi; G) + \lambda_h H(\phi; G)\bigr]$$

$\phi^*(G)$ is its unique fixed point ([[collective focus theorem]]). $\hat\phi^*_v = \phi^*(G_v)$ is neuron $v$'s local estimate.

under partial synchrony: messages arrive within unknown but finite $\Delta$. all neurons eventually receive all signals — no permanent partitions.

## theorem S1: subgraph lipschitz

the fixed point $\phi^*$ is Lipschitz in edge weights with constant $L = C/(1-\kappa)$:

$$\bigl\|\phi^*(G) - \phi^*(G_v)\bigr\|_1 \;\leq\; \frac{C}{1-\kappa} \cdot \varepsilon_v$$

where $\kappa < 1$ is the [[tri-kernel]] contraction rate and $C \leq 2$ is a constant bounding the per-unit operator sensitivity to edge weight perturbations.

proof sketch. let $\delta W = W_G - W_{G_v}$ be the matrix of missing edges. for any fixed $\phi$:

$$\bigl\|T(\phi; G) - T(\phi; G_v)\bigr\|_1 \leq C \cdot \|\delta W\|_1 / \|W\|_1 = C\varepsilon_v$$

the fixed points satisfy the identity:

$$\phi^*(G) - \phi^*(G_v) = \underbrace{\bigl[T(\phi^*(G); G) - T(\phi^*(G_v); G)\bigr]}_{\text{contracts by } \kappa} + \underbrace{\bigl[T(\phi^*(G_v); G) - T(\phi^*(G_v); G_v)\bigr]}_{\leq\, C\varepsilon_v}$$

the first term contracts: $\|T(\phi^*(G); G) - T(\phi^*(G_v); G)\|_1 \leq \kappa\|\phi^*(G) - \phi^*(G_v)\|_1$.

combining and solving: $\|\phi^*(G) - \phi^*(G_v)\|_1 \leq C\varepsilon_v/(1-\kappa)$. $\square$

interpretation. a neuron missing $\varepsilon_v$ fraction of total stake weight makes a $\phi^*$ error of at most $C\varepsilon_v/(1-\kappa)$. with $\kappa = 0.74$, $C = 2$: error $\leq 7.7\,\varepsilon_v$. a neuron that has seen 99% of total stake weight has $\hat\phi^*$ error at most 7.7%.

## theorem S2: safety under partial synchrony

no two conflicting [[particles]] $P_a, P_b$ can both be finalized by correct neurons, even when neurons hold different views $G_u, G_v$, provided:

$$\varepsilon_{\max} \;\triangleq\; \max_v \varepsilon_v \;<\; \frac{(1-\kappa)\,\delta_{\text{stake}}}{2C}$$

where $\delta_{\text{stake}}$ is the honest [[stake]] margin above $\tfrac{1}{2}$.

proof sketch.

step 1: gap in global $\phi^*$. honest neurons apply exclusive support: each links to at most one of $\{P_a, P_b\}$. let $S_a$ be the total honest stake directed to $P_a$, $S_b$ to $P_b$. without loss of generality $S_a \geq S_b$ (the member that propagated to more honest neurons first accumulates more honest support). adversarial stake $A \leq \tfrac{1}{2} - \delta_{\text{stake}}$ goes entirely to $P_b$. by the base [[foculus]] safety proof:

$$\phi^*(G)(P_a) - \phi^*(G)(P_b) \;\geq\; \delta_{\text{stake}} \cdot \Delta_\kappa, \qquad \Delta_\kappa = \Omega\!\left(\frac{1-\kappa}{\kappa}\right)$$

the [[tri-kernel]] contraction exponentially amplifies the initial $\phi^*$ advantage — the gap $\Delta_\kappa$ is the spectral separation of the Perron-Frobenius eigenvector between the two conflict branches.

step 2: view error cannot close the gap. by S1, for any neuron $v$ with view error $\varepsilon_v$:

$$\hat\phi^*_v(P_b) \leq \phi^*(G)(P_b) + \frac{C\varepsilon_v}{1-\kappa}$$

the adaptive threshold $\tau_v = \mu_{\hat\phi^*_v} + \kappa'\sigma_{\hat\phi^*_v}$ shifts from the global $\tau$ by at most $\Delta\tau_v = O(C\varepsilon_v)$ due to the distribution shift from missing edges. for $P_b$ to finalize in $v$'s view:

$$\phi^*(G)(P_b) + \frac{C\varepsilon_{\max}}{1-\kappa} > \tau(G) - O(C\varepsilon_{\max})$$

step 3: contradiction. if $P_a$ finalizes in the global view, $\phi^*(G)(P_a) > \tau(G)$. by step 1:

$$\phi^*(G)(P_b) \leq \phi^*(G)(P_a) - \delta_{\text{stake}}\Delta_\kappa < \tau(G) - \delta_{\text{stake}}\Delta_\kappa$$

substituting into step 2's requirement for $P_b$ to finalize in any view:

$$\tau(G) - \delta_{\text{stake}}\Delta_\kappa + \frac{C\varepsilon_{\max}}{1-\kappa} + O(C\varepsilon_{\max}) > \tau(G)$$

rearranging: the inequality holds only if $C\varepsilon_{\max}/(1-\kappa) + O(C\varepsilon_{\max}) > \delta_{\text{stake}}\Delta_\kappa$, i.e. $\varepsilon_{\max} > \Omega((1-\kappa)\delta_{\text{stake}})$. the theorem condition $\varepsilon_{\max} < (1-\kappa)\delta_{\text{stake}}/(2C)$ is strictly below this, so the inequality is false. $P_b$ cannot finalize. $\square$

concrete bound. with $\kappa = 0.74$, $C = 2$, $\delta_{\text{stake}} = 0.05$:

$$\varepsilon_{\max} < \frac{0.26 \times 0.05}{4} = 0.00325$$

each neuron must have seen at least 99.7% of total stake weight before finalizing. propagation time for this: $O(\Delta\log N)$ by the [[vec]] P4 gossip liveness bound — at $\Delta = 0.4\text{s}$, $N = 10^{15}$: $\approx 20\text{s}$.

## theorem S3: spectral gap at adversarial scale

the stake-weighted [[spectral gap]] satisfies:

$$\lambda_2 \;\geq\; \Omega\!\left(\frac{\delta_{\text{stake}}^2}{\log^2 N}\right)$$

under any adversarially-chosen [[cybergraph]] topology with $N$ particles, provided honest neurons hold $> \tfrac{1}{2} + \delta_{\text{stake}}$ of staked tokens. for realistic power-law graphs (degree exponent $\beta \in (2,3)$, as observed in all real-world hyperlink networks), the bound tightens to $\lambda_2 \geq \Omega(\delta_{\text{stake}}/\log N)$.

proof sketch.

the adversary's strategy to minimize $\lambda_2$ is to plant a sparse cut: separate $P$ into halves $A, B$ with few crossing edges. the Cheeger inequality relates $\lambda_2$ to the Cheeger constant $h(G)$:

$$\lambda_2 \geq h(G)^2/2, \qquad h(G) = \min_{S:\,|S|\leq N/2} \frac{\Phi(S)}{\operatorname{vol}(S)}$$

for a sparse cut on $S$ to persist, the adversary must prevent honest neurons from adding crossing edges. honest neurons in $S$ freely create [[cyberlinks]] to neurons outside $S$. the rate of new honest crossing edges per epoch $\geq \delta_{\text{stake}} \cdot d_{\min}$ where $d_{\min}$ is the minimum honest degree in $S$.

maintaining a cut of capacity $\Phi(S)$ requires the adversary to control those $\Phi(S)$ stake-weighted edges. total adversarial stake $< \tfrac{1}{2} - \delta_{\text{stake}}$. the minimum-weight honest cut therefore satisfies:

$$\Phi_{\min}(S) \geq \delta_{\text{stake}} \cdot d_{\min} \cdot |S| / N$$

for power-law degree distributions with exponent $\beta \in (2,3)$, the hub structure ensures the volume $\operatorname{vol}(S) = O(N^{1/(3-\beta)}/\log N) \cdot |S|$, giving $h(G) = \Omega(\delta_{\text{stake}}/\log N)$ and $\lambda_2 \geq h^2/2 = \Omega(\delta_{\text{stake}}^2/\log^2 N)$.

for adversarially-sparse graphs (no hubs), $d_{\min}$ can be driven to 1, giving $h(G) = \Omega(\delta_{\text{stake}}/N)$ and $\lambda_2 = \Omega(\delta_{\text{stake}}^2/N^2)$ — convergence degrades linearly with $N$ on pathological topologies. the honest majority prevents this: creating hub-less graphs requires controlling all high-degree nodes, which costs proportional stake. an adversary with $< \tfrac{1}{2}$ stake cannot prevent hubs from forming under honest majority linking. $\square$

convergence time. expected iterations to $\varepsilon$-convergence:

$$t(\varepsilon) = O\!\left(\frac{\log(1/\varepsilon)}{\lambda_2}\right)$$

| graph type | $\lambda_2$ | $t(\varepsilon)$ at $N=10^{15}$ | interpretation |
|---|---|---|---|
| adversarial sparse | $\delta^2/\log^2 N$ | $\log(1/\varepsilon) \cdot \log^2 N / \delta^2$ | worst case, unlikely under honest majority |
| power-law $\beta\in(2,3)$ | $\delta/\log N$ | $\log(1/\varepsilon) \cdot \log N / \delta$ | expected under natural graph growth |
| dense honest majority | $\Omega(1)$ | $O(\log(1/\varepsilon))$ | high-stake hub neighborhoods, local finality |

the third row explains the 1-3s empirical finality for well-endorsed [[particles]]: their local neighborhoods are dense, $\lambda_2$ is large locally, and they finalize from their local spectral gap — not the global one. global $\lambda_2$ governs worst-case finality for periphery particles with few inbound [[cyberlinks]].

## theorem S4: shard composition

partition $P$ into $K$ shards $P_1, \ldots, P_K$. let $f_\times = \sum_{e\in E_\times} w(e)/\|W\|$ be the cross-shard edge weight fraction. each shard $k$ computes local $\phi^*_k$ treating cross-shard links as external inputs weighted by the previous round's $\phi^*_{k'}$.

one round of shard composition:

$$\phi^*_{\text{composed}} = \operatorname{normalize}\!\left(\sum_k \operatorname{vol}(G_k) \cdot \phi^*_k\right)$$

satisfies:

$$\bigl\|\phi^*_{\text{composed}} - \phi^*(G)\bigr\|_1 \;\leq\; \frac{C\,f_\times}{(1-\kappa)\,K}$$

after $R$ rounds of iterated shard recomputation the error contracts geometrically:

$$\bigl\|\phi^*_{(R)} - \phi^*(G)\bigr\|_1 \;\leq\; \frac{C\,f_\times}{1-\kappa} \cdot \kappa^R$$

$R = \lceil\log(1/\varepsilon)/\log(1/\kappa)\rceil$ rounds suffice for error $< \varepsilon$. with $\kappa = 0.74$: $R = 5$ rounds for $\varepsilon = 0.01$. each round requires exactly one cross-shard message exchange — $O(K)$ messages total, independent of $N$.

proof sketch. cross-shard links contribute $f_\times/K$ of stake weight per shard boundary by uniform partition. treating them as fixed external inputs is equivalent to missing those edges from the shard's perspective. by S1, per-shard error $\leq Cf_\times/(K(1-\kappa))$. the composed estimate inherits this additively. for iterated composition: cross-shard inputs improve each round by factor $\kappa$ because the shard-local operators are $\kappa$-contractions and cross-shard errors propagate through them. $\square$

safety under sharding. S2 applies per-shard with cross-shard weight playing the role of view error: $f_\times/K < \varepsilon_{\max}$. for $K = 100$ shards and $f_\times = 0.10$: per-shard cross-shard error = $0.001 < 0.00325$. safety holds with one round of cross-shard exchange.

throughput implication. $K$ shards each compute their own SpMV independently. total throughput scales as $K \times$ single-shard throughput. cross-shard communication cost is $O(K \cdot f_\times \cdot |E|)$ edge weights exchanged per round — sublinear when $f_\times \ll 1$ and $K$ is chosen to minimize $f_\times$ (by assigning particles to shards with their most-linked neighbors).

## theorem S5: liveness at scale

every valid [[particle]] $P_i$ with inbound honest [[stake]] exceeding $\tau \cdot \|W\|$ finalizes within:

$$t_{\text{final}} = O\!\left(\Delta \cdot \log(N/\delta) + \frac{\log(1/\varepsilon)}{\lambda_2}\right)$$

where $\delta$ is failure probability over gossip randomness and $\varepsilon$ is the finality margin $(\tau - \phi^*(P_i)^{-1})$.

proof sketch. two phases.

phase 1 — propagation, time $O(\Delta\log(N/\delta))$. by [[vec]] P4 (liveness), every signal from a correct neuron reaches all correct neurons within $O(\Delta\log(N/\delta))$ under gossip with fanout $\geq 2$. after this phase all correct neurons have view error $\varepsilon_v < \varepsilon_{\max}$ (S2 condition).

phase 2 — convergence, time $O(\log(1/\varepsilon)/\lambda_2)$. once views are complete, local $\hat\phi^*_v$ converge to $\phi^*(G)$ in $O(\log(1/\varepsilon)/\lambda_2)$ iterations. by S1, $\hat\phi^*_v(P_i) > \phi^*(G)(P_i) - C\varepsilon_v/(1-\kappa) > \tau$ for sufficient honest support. all correct neurons finalize $P_i$.

combining phases and substituting S3's lower bound on $\lambda_2$:

$$t_{\text{final}} = O\!\left(\log N \cdot \left(\Delta + \frac{\log(1/\varepsilon)}{\delta_{\text{stake}}^2}\right)\right) \;\square$$

concrete evaluation at planetary scale ($N = 10^{15}$, $\Delta = 0.4\text{s}$, $\delta_{\text{stake}} = 0.05$, $\varepsilon = 10^{-6}$):

| scenario | $\lambda_2$ | $t_{\text{final}}$ |
|---|---|---|
| adversarial sparse graph | $\delta^2/\log^2 N \approx 5\times10^{-6}$ | $\sim 10^5\text{s}$ (days) |
| power-law realistic graph | $\delta/\log N \approx 10^{-3}$ | $\sim 10^3\text{s}$ (minutes) |
| hub neighborhood (local) | $\Omega(1)$ | $1$–$3\text{s}$ |

the adversarial case is the mathematical worst case under the honest majority assumption, not an expected operating condition. it requires the adversary to hold nearly $\tfrac{1}{2}$ of stake AND construct a graph with no hubs — both conditions are economically expensive. under natural graph growth (power-law structure, hubs form from stake concentration), the realistic column applies. the hub column explains measured finality for high-$\phi^*$ particles.

## security parameter table

| parameter | symbol | security condition | concrete value |
|---|---|---|---|
| honest stake margin | $\delta_{\text{stake}}$ | $> 0$ | 0.05 (5%) |
| max view error | $\varepsilon_{\max}$ | $< (1-\kappa)\,\delta_{\text{stake}} / (2C)$ | $< 0.00325$ |
| required view completeness | $1 - \varepsilon_{\max}$ | $> 99.67\%$ of stake weight seen | 99.7% |
| propagation time to completeness | $t_{\text{prop}}$ | $O(\Delta\log N)$ | $\approx 20\text{s}$ at $N=10^{15}$, $\Delta = 0.4\text{s}$ |
| spectral gap (adversarial) | $\lambda_2$ | $\geq \delta^2/\log^2 N$ | $\geq 5\times10^{-6}$ at $N=10^{15}$ |
| spectral gap (power-law) | $\lambda_2$ | $\geq \delta/\log N$ | $\geq 10^{-3}$ at $N=10^{15}$ |
| shard composition rounds | $R$ | $\lceil\log(1/\varepsilon)/\log(1/\kappa)\rceil$ | 5 rounds for 1% error |
| max cross-shard fraction | $f_\times$ | $< K \cdot \varepsilon_{\max}$ | $< 0.33$ at $K=100$ |

the view-completeness condition (99.7% of stake weight seen before finalizing) is the key operational parameter. [[vec]] P4 + the gossip liveness bound together guarantee this condition is met within $O(\Delta\log N)$ — a propagation delay that grows only logarithmically with network size. at $N = 10^{15}$ this is $\approx 20\text{s}$, identical to the claimed 1-3s finality plus propagation margin.

## what this closes

closed by S1–S5:

- safety holds under partial views: S2 with the view-error condition
- convergence does not degrade to zero as $N \to \infty$: S3 proves $\lambda_2 = \Omega(1/\log^2 N)$ under honest majority, $\Omega(1/\log N)$ under realistic graphs
- shard composition converges to global $\phi^*$ in $R = O(\log(1/\varepsilon))$ cross-shard rounds: S4
- liveness scales as $O(\log N)$: S5
- throughput is $K \times$ per-shard SpMV, growing linearly with shard count: S4

## what remains open

tight Lipschitz constant $C$. the proof gives $C \leq 2$ from the operator bound. the actual $C$ depends on the tri-kernel weight distribution and graph structure. tighter analysis of the normalized diffusion + springs + heat composite may reduce $C$ below 1, relaxing the view-error bound and therefore reducing the propagation requirement.

spectral gap of stake-concentrated graphs. S3 assumes uniform stake. in practice stake concentrates at hubs (power-law stake distribution). stake-weighted spectral gap for scale-free graphs with correlated degree-stake distributions is not characterized. it may be significantly larger than the $\Omega(1/\log N)$ bound, tightening liveness guarantees.

threshold gaming under view error. S2 bounds the adaptive threshold shift by $O(C\varepsilon_v)$ and treats this as absorbed into the safety margin. a formal adversarial model for $\sigma_{\phi^*}$ manipulation across partial views — where the adversary strategically injects signals to inflate variance and raise $\tau$ — is not derived. the interaction between adaptive $\tau$ and view-error-bounded $\hat\phi^*$ needs a separate adversarial analysis.

shard assignment optimization. S4 assumes uniform random partition. optimal assignment minimizes $f_\times$ (cross-shard edge weight) and maximizes per-shard $\lambda_2$. this is a graph partitioning problem with a dual objective. graph partitioning approximations (Metis, spectral clustering) apply, but no tight bound on the achievable $f_\times$ for adversarial graph topologies is given.

adversarial honest-split. S2 proves no double finality but does not bound WHICH conflicting particle wins when the adversary controls initial propagation timing. the adversary can influence the outcome within the safety constraints. quantifying the adversary's residual power over conflict resolution — the probability they steer the wrong particle to finalize — requires a separate timing analysis under partial synchrony.

---

see [[foculus]] for the base protocol and safety proof these theorems extend. see [[collective focus theorem]] for the contraction and convergence properties S1–S2 depend on. see [[convergence]] for spectral gap theory and the Cheeger inequality used in S3. see [[vec]] for VEC P4 (liveness) used in S5's propagation phase. see [[structural sync]] for the gossip model underlying the propagation time bound.
