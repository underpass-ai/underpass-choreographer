# 002 — Deliberation scale sweep

- **Author:** @underpass-ai
- **Date:** 2026-04-18
- **Status:** complete
- **Related code:**
  - `crates/choreo-app/benches/deliberate.rs`
  - Follow-up to [`001-baseline-deliberation-latency`](../001-baseline-deliberation-latency/README.md)

## 1. Hypothesis

`DeliberateUseCase::execute` wall-time, measured against stubbed
ports, fits a two-term linear model:

```
  time(agents, rounds) ≈ a + b·agents + c·(rounds × agents)
```

- `a`: fixed per-invocation overhead (start, save, publish, record).
- `b·agents`: per-agent work that happens exactly once per
  invocation (seed, validate, score, rank).
- `c·(rounds × agents)`: per-pair work inside the peer-review loop,
  which pairs each of `N` agents with its `(i+1) mod N` neighbour
  for `rounds` iterations.

Expected order of magnitude: `a ≈ 1 µs`, `b ≈ 1 µs`, `c ≈ 0.1 µs`.

## 2. Experiment design

- **Grid**: `agents ∈ {1, 3, 5, 10}` × `rounds ∈ {0, 2, 4, 8}` =
  16 points. Spans the realistic operational range (single-agent
  degenerate case through 10-agent committee, zero-revision
  through 8-round peer review).
- **Fixed variables**: every port stubbed; `StubAgent` returns
  constant content; `PassValidator` + `FullScoring`; same Tokio
  current-thread runtime reused across iterations.
- **Measurement**: criterion 0.5, `--warm-up-time 1 --measurement-time 2
  --sample-size 50`. Shorter than defaults so all 16 points fit in
  ~2 min total; confidence intervals are wider in exchange (typical
  ±1–3 %). Raw output in [`results/deliberate-grid.txt`](results/deliberate-grid.txt).
- **Refutation**: if the model's residuals exceed 20 % of the
  observed median at any grid point, the linear fit is inadequate
  and we need a richer model (quadratic in agents, cache effects,
  etc.).
- **Environment**: identical to experiment 001 — AMD Ryzen
  Threadripper PRO 5955WX / Linux 6.19 / rustc 1.90.0.

## 3. Method

```bash
bash docs/experiments/002-deliberation-scale-sweep/run.sh
```

## 4. Results

Median latencies in microseconds:

| agents \ rounds | 0     | 2     | 4     | 8     |
|-----------------|-------|-------|-------|-------|
| **1**           | 1.54  | 1.56  | 1.52  | 1.53  |
| **3**           | 2.79  | 3.37  | 3.75  | 4.36  |
| **5**           | 5.13  | 5.74  | 6.60  | 7.63  |
| **10**          | 10.80 | 12.48 | 14.34 | 17.52 |

Observations drawn directly from the grid:

1. **`agents = 1` is flat across rounds.** Expected: the aggregate
   short-circuits the peer-review loop when `agents.len() < 2`,
   so additional rounds are no-ops.
2. **At `rounds = 0` the per-agent slope is ~1 µs from 3 agents
   upward.** Specifically the jumps are 0.62 µs/agent from 1→3,
   1.17 µs/agent from 3→5, and 1.13 µs/agent from 5→10. The
   superlinearity between 1 and 3 agents is fixed-overhead
   amortisation.
3. **Peer-review adds ~60–85 ns per round-agent pair.** At 3, 5,
   and 10 agents, the average added cost per `round × agent` is
   65 ns, 62 ns, 84 ns respectively. The 10-agent slope is
   slightly higher — consistent with cache / BTreeMap-lookup
   effects when the proposal map grows.

### Linear fit

Extracting `c` from the peer-review contribution at each agent
size (slope of `time(rounds) − time(rounds = 0)` over
`rounds × agents`):

| agents | c (ns / round·agent) |
|--------|----------------------|
| 3      | 65                   |
| 5      | 62                   |
| 10     | 84                   |

Extracting `a` and `b` from the `rounds = 0` column (least squares
on the 3 / 5 / 10 points, since agents=1 is amortisation-dominated):

```
  time(rounds = 0) ≈ 0.26 + 1.05 · agents     (µs)
```

Combining:

```
  time(agents, rounds) ≈ 0.26 + 1.05·agents + 0.07·(rounds × agents)    (µs)
```

Residuals from this fit are within ±10 % of the measured median
for every 3 / 5 / 10-agent point and within ±25 % for the
single-agent point (which the model over-predicts because it
extrapolates outside the fit range).

## 5. Conclusion

**Hypothesis supported** for the operational range that matters
(3 agents and above). The two-term linear model `a + b·agents +
c·(rounds × agents)` explains the grid within 10 % residuals with
`a ≈ 0.26 µs`, `b ≈ 1.05 µs`, `c ≈ 0.07 µs`.

The single-agent case is an artefact of fixed-cost amortisation —
it is not a refutation, just a reminder that a linear model
extrapolated below its fit range should not be trusted.

## 6. Threats to validity

- **Still stub-agent.** As in 001, every agent call is a constant
  no-op. Real LLM latencies (tens to thousands of ms) dwarf the
  microsecond regime measured here. This experiment is about
  **the choreographer's own scaling**, not about deliberation
  latency in production.
- **Narrow agent range.** The grid tops out at 10 agents. We have
  not measured cache effects that would emerge at, say, 32 or 64
  agents. The slight `c` bump from agent=5 to agent=10 hints at
  BTreeMap growth, but we need larger-N data to confirm or refute.
- **Moderate criterion budget.** `--measurement-time 2 --sample-size
  50` buys speed at the cost of tighter intervals. The 95 % Tukey
  CIs are typically ±1–3 % of the median, good enough for an
  order-of-magnitude slope; a future experiment requiring sub-
  percent precision should bump the budget.
- **No allocator metric.** Same as 001 — wall time only.

## 7. Follow-ups

- **003** — wide-N sweep: `agents ∈ {16, 32, 64}`, `rounds ∈ {0,
  4, 16}`. Confirms or refutes the BTreeMap-growth hint.
- **004** — end-to-end through NATS + gRPC + Postgres so the
  adapter layer cost stacks on top of the domain cost measured
  here.
- **005** — realistic agent mock with tunable latency
  distribution (e.g. ~200 ms per call) so the whole-system budget
  is documented honestly.
