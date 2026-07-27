# Contributing

This repository is the Underpass Choreographer. Contributions should
keep it independently usable, use-case agnostic, provider-agnostic, and
API-first.

Start here:

- `docs/PRINCIPLES.md` for the engineering rules this repo enforces.
- `docs/dev-loop.md` for setup and local commands.
- `.github/pull_request_template.md` for the PR checklist.
- `SECURITY.md` for vulnerability reporting and secret handling.
- `CHANGELOG.md` for unreleased release notes.

## What Belongs Here

Choreographer-owned work includes:

- gRPC contracts under `crates/choreo-proto/proto/underpass/choreo/v1`;
- AsyncAPI event contracts under `specs/asyncapi`;
- council, agent, deliberation, output-contract, and orchestration
  behavior;
- MCP exposure of the Choreographer gRPC surface;
- provider adapter boundaries that stay behind `AgentPort`;
- Runtime executor integration as an optional executor adapter;
- container image, Helm chart, and repo-owned smoke/E2E tooling.

Work that belongs elsewhere:

- KMP, PIR, Runtime, or product-specific business workflows;
- domain vocabulary in core contracts or chart defaults;
- provider-specific behavior in `choreo-core`;
- deployment credentials, private endpoints, customer data, or secrets.

Use `attributes`, `payload`, or `google.protobuf.Struct` for
deployment-specific metadata. Do not add typed public contract fields
for one product or one use case.

## Local Setup

Use the setup in `docs/dev-loop.md`. Minimum tools:

- Rust `1.97.1`;
- `just`;
- `buf`;
- AsyncAPI CLI;
- `protoc`;
- Docker or Podman for container-backed gates.

The smallest local run has no external dependencies:

```bash
CHOREO_NATS_ENABLED=false just run
```

Without `just`:

```bash
CHOREO_NATS_ENABLED=false cargo run --locked -p choreo
```

## Development Workflow

1. Sync from `main`.
2. Create a branch with a narrow scope.
3. Make the contract change first when public API or event shape
   changes.
4. Add or update tests before relying on the behavior in docs.
5. Update docs and `CHANGELOG.md` when the user-facing surface changes.
6. Run the relevant gates locally.
7. Open a PR using `.github/pull_request_template.md`.

Prefer small PRs. If a change touches contracts, runtime behavior, the
Helm chart, and docs at once, explain why that coupling is necessary.

## Required Gates

Before opening a normal PR:

```bash
just check
just helm-lint
```

Equivalent without `just`:

```bash
bash scripts/ci/quality-gate.sh
bash scripts/ci/helm-lint.sh
```

Run container-backed integration when touching NATS, Postgres,
persistence, messaging, container runtime behavior, or testcontainers
setup:

```bash
just integration
```

Run full E2E before release work or when changing compose, Kubernetes
jobs, Helm install behavior, provider-shaped E2E, or cross-service
smoke paths:

```bash
make e2e-compose
make e2e-kubernetes
```

Run consumer smoke when changing the public gRPC behavior that
downstream consumers rely on:

```bash
cargo run -p choreo-consumer-smoke --locked -- \
  --endpoint http://127.0.0.1:50055 \
  --chain all
```

If a gate cannot be run locally, say so in the PR and explain what was
run instead.

## Contract Changes

Choreographer is API-first:

- gRPC source: `crates/choreo-proto/proto/underpass/choreo/v1/choreo.proto`;
- AsyncAPI source: `specs/asyncapi/choreographer.asyncapi.yaml`.

For public-surface changes:

1. Update the spec first.
2. Regenerate code if generated files are affected.
3. Update MCP parity if the gRPC surface changes.
4. Update docs and examples.
5. Run `just contract` or `bash scripts/ci/contract-gate.sh`.

Breaking changes must be intentional and called out in the PR. The
contract gate compares against `origin/main` when available.

## Testing Expectations

Match test scope to risk:

- Pure domain logic: unit tests in the owning crate.
- Adapter logic: adapter tests plus integration tests where the external
  boundary matters.
- gRPC/MCP surface: proto/API tests and MCP parity tests.
- Event shape: AsyncAPI validation and event-shape tests.
- Helm changes: `bash scripts/ci/helm-lint.sh` plus render assertions
  for any new hardening or failure path.
- User workflows: consumer smoke, compose E2E, or Kubernetes E2E.

Do not document a behavior as supported until a committed test, smoke,
or operator command proves it.

## Documentation

Update docs in the same PR as behavior changes:

- Add new operational docs to `docs/index.md`.
- Add release-note entries to `CHANGELOG.md` under `Unreleased`.
- Update `README.md` only for first-contact or product-surface changes.
- Keep research and case-study docs clearly labeled as such.
- Keep KMP, PIR, Runtime, and other project references framed as
  optional use cases or integrations unless the code truly requires
  them.

Use factual language. Performance, coverage, quality, and hardening
claims need a gate, experiment, benchmark, or reproducible command in
the repository.

## Security Rules

- Never commit secrets, tokens, private keys, private endpoints, or
  customer data.
- Do not place provider credentials in agent descriptors or values
  files.
- Use Kubernetes Secrets, secret managers, or `valueFrom.secretKeyRef`
  for sensitive deployment input.
- Report vulnerabilities through `SECURITY.md`, not public issues with
  exploit details.
- If a change affects TLS/mTLS, secret rendering, provider credentials,
  container hardening, or NetworkPolicy, add a chart/test gate that
  would fail if the security property regressed.

## Commit and PR Style

Prefer concise, imperative commits. Existing history commonly uses:

- `feat(scope): ...`
- `fix(scope): ...`
- `docs(scope): ...`
- `test(scope): ...`
- `chore: ...`

PR descriptions should lead with what changed and why it belongs in the
Choreographer. Include the gates run and any gate you could not run.

## Release Work

Releases are maintainer-owned and follow `docs/release.md`.

Do not create or move `v*` tags manually. Use:

```bash
just version X.Y.Z
just release X.Y.Z
```

Only release from merged `main` after the release checklist gates pass.
