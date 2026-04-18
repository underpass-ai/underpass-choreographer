# Experiments

This directory is the lab notebook of the Underpass Choreographer.

Every behavioural, performance, or quality claim in this project must be
backed by a reproducible experiment recorded here. We do **not** ship
marketing numbers. If an experiment has not been run yet, the claim does
not appear in the docs, the README, the commit message, or the release
notes.

## Format

Each experiment is a directory `docs/experiments/NNN-short-slug/`
containing at least:

```
docs/experiments/NNN-short-slug/
├── README.md          # the write-up (template below)
├── run.sh             # reproducer: everything needed to re-run locally / in CI
└── results/           # raw outputs, logs, charts, summary.json
```

Numbering is monotonic and global. Experiments are append-only —
superseded conclusions get a new experiment, not an edit.

## Write-up template

Copy this verbatim into the experiment's `README.md`:

```markdown
# NNN — <title>

- **Author:**
- **Date:**
- **Status:** draft | running | complete | superseded-by-NNN
- **Related code:** <commit hashes / PRs / crates touched>

## 1. Hypothesis
One sentence. Falsifiable. What do we believe will happen and why?

## 2. Experiment design
- Inputs, knobs, fixed variables
- What is measured, how, with what tolerance
- What would count as a refutation of the hypothesis
- Sample size / number of runs
- Environment (kernel, CPU, container runtime, versions)

## 3. Method
Step-by-step. Must match `run.sh`. A reader should be able to re-derive
the results without asking questions.

## 4. Results
Raw numbers. Include uncertainty (stddev, CI) where it matters.
Charts allowed but numbers are canonical.

## 5. Conclusion
Did the data support the hypothesis? Be honest about what we did *not*
establish. Null results and refutations are kept — do not delete them.

## 6. Threats to validity
What could be wrong with this experiment? Confounders, small N, biased
workload, measurement artefacts.

## 7. Follow-ups
Concrete next experiments (by number or slug).
```

## Index

| # | Slug | Status | Summary |
|---|---|---|---|
| 001 | [baseline-deliberation-latency](001-baseline-deliberation-latency/README.md) | complete | `DeliberateUseCase::execute` runs in ~3.1–3.6 µs (3 agents, 0–2 rounds) on commodity hardware with stubbed ports — domain loop stays well under a 10 µs self-cost budget. |
| 002 | [deliberation-scale-sweep](002-deliberation-scale-sweep/README.md) | complete | 4×4 grid (agents ∈ {1,3,5,10} × rounds ∈ {0,2,4,8}). Two-term linear model `0.26 + 1.05·agents + 0.07·(rounds × agents)` µs fits every operational point within ±10 %. Peer-review costs ~60–85 ns per round-agent pair. |
