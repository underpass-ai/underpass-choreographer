# Consumer-smoke harness

`choreo-consumer-smoke` is a CLI that drives the Choreographer's
public surface the way a real downstream consumer would. It does not
share any in-process types with the choreographer's own runtime — it
only talks gRPC over `tonic` and (optionally) core NATS over
`async-nats`. That makes it a faithful smoke test of the integration
contract a real consumer commits to.

Two chains run, both shipped today:

- **Chain 1** — Warn-mode reevaluation. Mirrors what a consumer
  triggers after observing an incident or domain event: optionally
  publish a trigger envelope, invoke `RunCouncilDecision` in Warn
  mode with a kernel-rehydration-shaped bundle, then assert on the
  typed response + the outbound `choreo.deliberation.completed`
  envelope (correlation / causation propagation).

- **Chain 2** — Strict-mode handoff report. Registers the canonical
  Report `OutputContract` (JSON Schema body bound in
  `OutputContract.json_schema`), invokes `RunCouncilDecision` in
  Strict mode, and asserts that the choreographer either accepts a
  schema-conformant response (positive path) or rejects free-form
  text with `Code::FailedPrecondition` whose message mentions the
  contract id (rejection path — the path today's NoopAgent stack
  reaches).

## Prerequisites

- A running Choreographer (e.g. `make e2e-compose`, or a live
  cluster you can reach over gRPC).
- A seeded council under the target specialty (default `triage`)
  with at least one agent registered. Without a council the gRPC
  call returns `Code::NotFound` and Chain 1 records every assertion
  as Failed.
- For Chain 2: a writable contract registry. The chain calls
  `RegisterContract` and tolerates `AlreadyExists` /
  `FailedPrecondition` (already seeded) as a pass — but the registry
  must accept new contracts when starting from empty.

For the canonical bus subjects (Chain 1's NATS-coupled assertions):

- Trigger: `choreo.trigger.<specialty>`
- Deliberation completed: `choreo.deliberation.completed`

If `--nats-url` is omitted, those assertions are recorded as
`Skipped` (never silently dropped) and the rest of the chain still
runs.

## Invocation

```bash
cargo run -p choreo-consumer-smoke -- \
    --endpoint http://localhost:50055 \
    [--nats-url nats://localhost:4222] \
    [--chain {one,two,all}] \
    [--specialty triage] \
    [--contract-id consumer-smoke-report-v1]
```

Environment overrides:

- `CHOREOGRAPHER_ENDPOINT` — defaults to `http://localhost:50055`.
- `CHOREO_NATS_URL` — optional. When set, Chain 1 publishes the
  trigger envelope and subscribes to `choreo.deliberation.completed`
  for the correlation/causation assertions.
- `CHOREO_REPORT_SCHEMA_PATH` — Chain 2 reads the schema from this
  path. Default `api/examples/output-contracts/report.schema.json`
  (relative to the binary's cwd).
- `RUST_LOG` — standard tracing filter. Default `info`.

A `make consumer-smoke` target wraps the same call:

```bash
make consumer-smoke
CONSUMER_SMOKE_CHAIN=two CHOREOGRAPHER_ENDPOINT=https://staging:50055 \
    make consumer-smoke
```

## What each chain asserts

| Chain | Assertion | Pass when |
|-------|-----------|-----------|
| chain1 | `rpc_returned_winner` | `response.winner` is `Some` |
| chain1 | `validation_summary_present` | `response.validation` is `Some` |
| chain1 | `candidates_non_empty` | `response.candidates.len() > 0` |
| chain1 | `bundle_seam_documented` | always `Skipped` — points at Epic 11 scenario 7 (bundle round-trip) |
| chain1 | `trigger_envelope_observed` | a `choreo.deliberation.completed` envelope with the run's `correlation_id` arrives within 5 s |
| chain1 | `causal_metadata_propagated` | that envelope's `causation_id` matches the one the harness sent |
| chain2 | `report_schema_registered` | `RegisterContract` succeeds or the contract already exists |
| chain2 | `report_contract_rejects_freeform_text` | `RunCouncilDecision` returns `FailedPrecondition` mentioning the contract id |
| chain2 | `report_payload_validates` | the winner's content satisfies the schema (positive path) |

Each assertion is also typed (`Passed` / `Skipped { reason }` /
`Failed { detail }`), so callers that embed the library can assert on
the typed shape without parsing the printed table.

## Known limitations

- **`bundle_seam_documented` is intentionally `Skipped`.** The
  stack-level external context bundle round-trip is covered by
  `make e2e-compose` scenario 7; this consumer harness keeps that
  assertion as a documented out-of-scope seam rather than duplicating
  the stack E2E.
- **Chain 2's positive path needs a structured-JSON agent.** The
  default smoke path targets a NoopAgent stack and proves strict
  schema rejection. The compose stack also ships a `stub-llm` sidecar
  that emits Report-shaped JSON; wiring the consumer smoke harness to
  register that agent is a follow-up positive-path mode.
- The trigger publish in Chain 1 is informational only —
  `RunCouncilDecision` is invoked directly, so the trigger path does
  not gate the run. Pinning the trigger-driven path end-to-end is
  Epic 11's territory.

## Exit codes

- `0` — every selected chain passed (at least one `Passed`
  assertion, no `Failed`).
- `1` — at least one chain recorded a `Failed` assertion.
- `2` — infrastructure error: could not connect to the gRPC endpoint
  within the configured budget, or a chain runner returned `Err`
  (typically a panicking dependency).

## The kernel rehydration seam

`choreo_consumer_smoke::bundle::deterministic_bundle()` returns a
literal `ExternalContextBundle`. A real consumer integration would
replace it with the result of a kernel rehydration call (Underpass
KMP, RAG, whatever the consumer wires) before invoking
`RunCouncilDecision`:

```text
  let bundle = kernel.rehydrate(...).await?;
  harness.grpc
      .run_council_decision(req.with_external_context(bundle))
      .await?;
```

Keeping the rehydration adapter out of this crate keeps the smoke
binary's dependency surface narrow. The chains exercise the
choreographer's public RPC + bus contract; the kernel boundary is a
separate integration concern.
