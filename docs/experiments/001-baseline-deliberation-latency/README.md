# 001 — Baseline deliberation latency

- **Author:** @underpass-ai
- **Date:** 2026-04-18
- **Status:** complete
- **Related code:**
  - `crates/choreo-app/benches/deliberate.rs`
  - `crates/choreo-core/benches/trace_context.rs`

## 1. Hypothesis

A single deliberation through `DeliberateUseCase` — with stub agents
and every port replaced by an in-process no-op — completes in
**under 10 µs** on commodity hardware for the canonical shape
(3 agents, 0 or 2 rounds). The hot path the choreographer owns
(seed → peer-review → validate → score → rank → save → publish →
record statistics) should not exceed that budget; anything slower
means the domain loop has acquired accidental overhead rather than
work.

Supporting micro-hypothesis: the W3C `TraceContext` wire-shape
helpers (`parse`, `to_header`, `generate`) stay in the sub-µs
regime; they are on every NATS publish and subscribe path, so a
regression there becomes a tax on the messaging fast path.

## 2. Experiment design

- **Inputs**: two parameterised shapes of `DeliberateUseCase::execute`:
  - `3-agents-0-rounds` — minimal loop, no revision.
  - `3-agents-2-rounds` — exercises the revision loop twice.
  Plus four `TraceContext` micro-benchmarks.
- **Fixed variables**: every adapter stubbed in-process, stub agent
  methods return constant content, `PassValidator` + `FullScoring`
  (all proposals pass, everyone scores 1.0), no I/O.
- **Measurement**: criterion 0.5 defaults (3-second measurement
  window, 100 samples, auto-calibrated iteration count, variance
  reported as a Tukey-style 95 % confidence interval on the median).
- **Refutation**: either benchmark's median exceeds 10 µs.
- **Environment**:
  - CPU: AMD Ryzen Threadripper PRO 5955WX (16 cores)
  - Kernel: Linux 6.19.8-arch1-1
  - Toolchain: rustc 1.90.0 (1159e78c4 2025-09-14)
  - Profile: `bench` (release-equivalent, LTO thin, codegen-units 1)

## 3. Method

```bash
bash docs/experiments/001-baseline-deliberation-latency/run.sh
```

The script runs both benches with criterion defaults and dumps the
raw output into `results/`. No warmup-time or sample-size overrides
are used — defaults are the source of truth.

## 4. Results

Medians with 95 % Tukey intervals from criterion's native reporter.

### `DeliberateUseCase::execute`

| Scenario              | Low       | Median   | High      |
|-----------------------|-----------|----------|-----------|
| 3-agents-0-rounds     | 3.100 µs  | 3.121 µs | 3.147 µs  |
| 3-agents-2-rounds     | 3.438 µs  | 3.577 µs | 3.722 µs  |

Raw criterion output: [`results/deliberate.txt`](results/deliberate.txt).

### `TraceContext` helpers

| Operation                        | Low       | Median    | High      |
|----------------------------------|-----------|-----------|-----------|
| `parse(valid)`                   | 84.15 ns  | 85.54 ns  | 87.24 ns  |
| `to_header`                      | 108.49 ns | 109.94 ns | 111.66 ns |
| `generate`                       | 424.33 ns | 430.15 ns | 436.69 ns |
| `parse + to_header` (round-trip) | 219.45 ns | 220.95 ns | 222.61 ns |

Raw criterion output: [`results/trace_context.txt`](results/trace_context.txt).

## 5. Conclusion

**Hypothesis supported.** The domain loop lands well under budget:

- `3-agents-0-rounds`: **~3.1 µs**, ~3× below the 10 µs bound.
- `3-agents-2-rounds`: **~3.6 µs**, still ~3× below; the revision
  loop adds ~450 ns (≈14 %) for two full agent rounds, which is
  consistent with doing O(rounds × agents) small stub-agent calls.
- `TraceContext::generate` at ~430 ns is the only helper above the
  100 ns band, dominated by two `uuid::v4` generations + hex
  encoding. Bearable on NATS publish (one per message), and the
  parse+format round-trip stays at 220 ns.

The supporting micro-hypothesis is also supported: every
`TraceContext` helper stays sub-µs.

## 6. Threats to validity

- **Stub agents hide the dominant real cost.** In production an
  agent call is an LLM HTTP round-trip (10–10 000 ms). This bench
  measures the choreographer's *own* cost only — it is the
  right question for "how much do we add?" but not the answer to
  "how long does a deliberation take?". Real-agent latencies will
  need their own experiment.
- **Small N of shapes.** We only bench two points (0 and 2 rounds,
  3 agents). The O(rounds × agents) peer-review loop should scale
  linearly by construction but we have not measured that.
- **Single host, single build.** No cross-machine comparison, no
  cross-compiler (stable only). The absolute numbers are not
  transferable; the *budget* is.
- **No memory metric.** Criterion reports wall time only. A leak
  or allocator regression would not surface here.

## 7. Follow-ups

- **002** — scale experiment: sweep `rounds ∈ {0, 2, 4, 8}` and
  `agents ∈ {1, 3, 5, 10}` to confirm the linear model.
- **003** — end-to-end through NATS + gRPC: puts the choreographer's
  ~3 µs in context next to the adapter + transport layer.
- **004** — real-agent mix: parameterise with a mock HTTP agent
  stubbing realistic LLM latencies so the whole-system budget is
  documented honestly.
