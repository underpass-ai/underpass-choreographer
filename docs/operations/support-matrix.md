# Support Matrix

This page records what the repository actually supports today. A
support claim belongs here only when the source of truth and the
enforcement gate are both named.

Choreographer remains independently usable. KMP, PIR, Runtime, provider
endpoints, and downstream products may be useful integration cases, but
they do not define this repository's support matrix unless this chart,
binary, or API requires them.

## Rust Toolchain

Current support is exact-version support, not a broad Rust range.

| Surface | Supported | Source of truth | Enforcement |
|---|---:|---|---|
| Workspace MSRV | `1.97` | `Cargo.toml` `[workspace.package].rust-version` | Cargo package metadata |
| Local toolchain | `1.97.1` | `rust-toolchain.toml` | `cargo`, `clippy`, `rustfmt` use the pinned toolchain |
| PR CI toolchain | `1.97.1` | `.github/workflows/quality-gate.yml` | `rustfmt`, `clippy`, tests, benches compile |
| Container-backed CI toolchain | `1.90.0` | `.github/workflows/integration.yml` | NATS and Postgres integration workflows |
| Developer setup | `1.90.0` | `docs/dev-loop.md` | Manual local setup and `just` recipes |
| Dependency resolution | `Cargo.lock` committed | `Cargo.lock` | CI and local gates use `--locked` |

### Supported

- Rust `1.90.0` with the components in `rust-toolchain.toml`:
  `clippy` and `rustfmt`.
- The locked dependency graph in `Cargo.lock`.
- The provider-feature matrix used by CI:
  `choreo-adapters/agent-anthropic`,
  `choreo-adapters/agent-openai`, and
  `choreo-adapters/agent-vllm`.

### Not Supported

- Older Rust versions.
- Nightly-only compiler features.
- "Latest stable" as a moving target.
- Builds that require dependency resolution different from the
  committed `Cargo.lock`.

### Change Rule

Changing the supported Rust version requires one PR that updates all of
these together:

- `Cargo.toml`;
- `rust-toolchain.toml`;
- `.github/workflows/quality-gate.yml`;
- `.github/workflows/integration.yml`;
- `docs/dev-loop.md`;
- this support matrix;
- `CHANGELOG.md`.

That PR must run:

```bash
just check
just helm-lint
just integration
```

Release-candidate work should also run:

```bash
make e2e-compose
make e2e-kubernetes
```

Do not merge a Rust version bump based only on local `cargo check`.

## Container Image Tags

Published image repositories:

| Image | Purpose | Build source | Publish path |
|---|---|---|---|
| `ghcr.io/underpass-ai/underpass-choreographer` | Product runtime image | `Dockerfile` | `.github/workflows/publish-distribution.yml` |
| `ghcr.io/underpass-ai/underpass-choreographer-e2e-runner` | Release/E2E runner image | `tests/e2e/runner.Dockerfile` | `.github/workflows/publish-distribution.yml` |

The provider-E2E image built by `scripts/ci/build-provider-image.sh`
is operator-pushed test tooling. It is not a supported production
runtime image.

| Reference form | Produced by | Support status | Production use |
|---|---|---|---|
| `image@sha256:<digest>` | Registry content digest | Supported and preferred | Yes. Use for normal Helm installs. |
| `image:vX.Y.Z` | `v*` tag publish workflow | Supported release label after the tag exists | Acceptable when release immutability is controlled; digest is still preferred. |
| `image:sha-<short>` | publish workflow commit tag | Supported for CI, release-candidate smoke, and cluster verification | Use only when the rollout records the source commit; digest is preferred. |
| `image:main` | default-branch publish workflow | Moving branch pointer | No. Development or smoke only. |
| `image:latest` | default-branch publish workflow | Moving branch pointer | No. The chart rejects it unless `development.allowMutableImageTags=true`. |
| `image:e2e-latest` | E2E runner publish workflow | Moving E2E runner pointer | No. Test runner only. |
| `image:dev`, `image:ci`, `image:e2e` | local scripts and test manifests | Local-only | No. Local compose/kind/k3d only. |

### Helm Image Rules

- `image.digest` wins over `image.tag`.
- Either `image.digest` or `image.tag` must be set; the chart has no
  production default image reference.
- `image.tag=latest` fails chart rendering unless
  `development.allowMutableImageTags=true`.
- `docs/operations/deploy-kubernetes.md` uses digests for normal
  installs and local tags only with an explicit development escape
  hatch.
- `scripts/ci/helm-lint.sh` asserts the missing-image and mutable-tag
  failure paths.

### Change Rule

Changing image tag support requires updating:

- `.github/workflows/publish-distribution.yml`;
- `scripts/ci/container-image.sh`, if local build semantics change;
- `charts/choreographer/templates/_helpers.tpl`, if chart acceptance
  changes;
- `scripts/ci/helm-lint.sh`;
- `docs/operations/deploy-kubernetes.md`;
- this support matrix;
- `CHANGELOG.md`.

## Helm Chart Versions

Chart source and release registry:

| Surface | Current value | Source of truth | Enforcement |
|---|---:|---|---|
| Chart path | `charts/choreographer` | repository layout | `scripts/ci/helm-lint.sh` |
| Chart name | `choreographer` | `charts/choreographer/Chart.yaml` | `helm lint` |
| Chart version | `0.1.0` | `charts/choreographer/Chart.yaml` `version` | `scripts/release.sh release` |
| App version | `0.1.0` | `charts/choreographer/Chart.yaml` `appVersion` | `scripts/release.sh release` |
| Kubernetes version floor | `>=1.28.0-0` | `charts/choreographer/Chart.yaml` `kubeVersion` | Helm client compatibility check |
| OCI registry | `oci://ghcr.io/underpass-ai/charts/choreographer` | `.github/workflows/publish-distribution.yml` | `helm package` + `helm push` |

| Chart reference | Support status | Notes |
|---|---|---|
| Checkout chart at `charts/choreographer` | Supported for development, PR review, and release-candidate validation | Must pass `bash scripts/ci/helm-lint.sh`. |
| `oci://ghcr.io/underpass-ai/charts/choreographer:0.1.0` | Pending | `0.1.0` is present in metadata but no public `v0.1.0` tag exists in this checkout yet. |
| `oci://ghcr.io/underpass-ai/charts/choreographer:X.Y.Z` | Supported after the matching `vX.Y.Z` release tag publishes successfully | Chart `version`, `appVersion`, and workspace version must match. |
| Older chart versions | Not currently supported | No stable-release support window has been declared yet. |
| Unversioned or moving chart references | Not supported | Use an explicit chart version. |

### Version Lockstep

The release helper keeps these values in lockstep:

- `Cargo.toml` `[workspace.package].version`;
- `charts/choreographer/Chart.yaml` `version`;
- `charts/choreographer/Chart.yaml` `appVersion`;
- Git tag `vX.Y.Z`;
- published product image tag `vX.Y.Z`;
- published E2E runner image tag `vX.Y.Z`;
- OCI chart version `X.Y.Z`.

Do not publish or document a chart version whose `appVersion` does not
match the binary/image version it is meant to deploy.

### Change Rule

Changing chart version support requires updating:

- `charts/choreographer/Chart.yaml`;
- `scripts/release.sh`;
- `.github/workflows/publish-distribution.yml`;
- `docs/release.md`;
- `docs/operations/deploy-kubernetes.md`, if install commands change;
- this support matrix;
- `CHANGELOG.md`.

## Provider Adapters

Provider support has three separate gates:

1. The binary must be compiled with the provider feature.
2. Required `CHOREO_*` environment variables must be present at boot.
3. The registered agent must use a kind listed in the startup
   `agent_kinds=...` log field.

Credentials must stay in environment or secret-managed files. Agent
descriptors may be persisted and must not carry credentials.

| Kind | Cargo feature | Default product image | Required env | Optional env | Repo validation | Support status |
|---|---|---:|---|---|---|---|
| `noop` | none | yes | none | none | unit tests, local smoke, minimal Helm smoke | Always supported. |
| `openai` | `choreo-adapters/agent-openai` | yes | `CHOREO_OPENAI_API_KEY` | `CHOREO_OPENAI_MODEL`, `CHOREO_OPENAI_ENDPOINT`, `CHOREO_OPENAI_MAX_TOKENS` | CI compile/test, compose scenario with OpenAI-compatible stub, consumer positive-path | Supported adapter shape. Real provider credentials, quotas, endpoint policy, and model behavior are operator-owned. |
| `vllm` | `choreo-adapters/agent-vllm` | yes | `CHOREO_VLLM_MODEL`, `CHOREO_VLLM_ENDPOINT` | `CHOREO_VLLM_BEARER_TOKEN`, `CHOREO_VLLM_MAX_TOKENS`, `CHOREO_VLLM_TIMEOUT_SECS` | CI compile/test, compose scenario with OpenAI-compatible stub, provider-E2E runner for real vLLM, gRPC council runner, MCP council runner (operator-run real execution pending) | Supported adapter shape. Service factory Helm path supports endpoint/model/bearer; vLLM client cert envs are provider-E2E runner only today. |
| `anthropic` | `choreo-adapters/agent-anthropic` | no | `CHOREO_ANTHROPIC_API_KEY` | `CHOREO_ANTHROPIC_MODEL`, `CHOREO_ANTHROPIC_ENDPOINT`, `CHOREO_ANTHROPIC_MAX_TOKENS` | CI compile/test | Implemented feature, not included in the default Dockerfile, and not covered by repo-owned E2E yet. Operators need a downstream image that enables the feature. |

Per-agent descriptor attributes supported by provider adapters:

| Attribute | Type | Meaning |
|---|---|---|
| `provider.model` | string | Override the env/default model for this agent. |
| `provider.endpoint` | string | Override the env/default endpoint for this agent. |
| `provider.max_tokens` | number, non-negative `u32` | Override token budget for this agent. |

### Image Feature Matrix

| Artifact | Provider features enabled |
|---|---|
| Default product `Dockerfile` | `agent-openai`, `agent-vllm` |
| `just check` / `scripts/ci/quality-gate.sh` | `agent-anthropic`, `agent-openai`, `agent-vllm` |
| Compose E2E stack | OpenAI-compatible and vLLM-compatible stub paths |
| Provider-E2E runner image | real `agent-vllm` runner path |
| MCP council runner | real `agent-vllm` path through `choreo-mcp` stdio; operator-run execution pending |

### Change Rule

Changing provider support requires updating:

- `crates/choreo-adapters/Cargo.toml`;
- `crates/choreo-adapters/src/agents/factory.rs`;
- `Dockerfile`, if the default product image feature set changes;
- `justfile` and `scripts/ci/quality-gate.sh`, if CI feature coverage
  changes;
- `docs/operations/deploy-kubernetes.md`;
- this support matrix;
- `CHANGELOG.md`.

If a provider is described as end-to-end supported, add or point to a
repo-owned smoke path that proves the provider shape without leaking
credentials.

## Kubernetes Posture

The Helm chart supports multiple deployment postures. "Supported" here
means the chart renders the posture, the binary has the corresponding
code path, and `scripts/ci/helm-lint.sh` or an operator smoke covers
the shape.

| Posture | Values / knobs | Status | Notes |
|---|---|---|---|
| Minimal standalone | `values.minimal.yaml` | Supported first-install smoke | No NATS, no Postgres, no Runtime, no provider credentials, no gRPC TLS. Not a hardened internet-facing posture. |
| Standalone with embedded NATS | `values.embedded-nats.yaml` | Supported standalone event bus | Release-local core NATS, no JetStream storage by default, no external NATS operation required. |
| Postgres persistence from Secret | `values.postgres-secret.yaml` | Supported for current persistent repositories | Persists deliberations, councils, agents, and statistics. Output contracts are still registered/seeded separately. |
| Provider env secrets | `values.provider-env-secrets.yaml`, `providerEnv`, `providerEnvFrom` | Supported env wiring | Secret injection only. Provider features and required env still gate runtime availability. |
| gRPC server TLS | `tls.mode=server` + `tls.existingSecret` | Supported | Secret must contain `tls.crt` and `tls.key`; health endpoints remain plaintext HTTP in-cluster. |
| gRPC mutual TLS | `tls.mode=mutual` + `tls.existingSecret` | Supported | Secret must contain `tls.crt`, `tls.key`, and client CA `ca.crt`. |
| Runtime executor | `executor.kind=runtime` + endpoint | Supported optional executor | Chart fails render without endpoint. Use only when Runtime is actually deployed. |
| Runtime client TLS/mTLS | `executor.runtime.tls.*` | Supported | Chart fails render for non-disabled Runtime TLS without an existing Secret. |
| NetworkPolicy | `networkPolicy.enabled=true` | Supported opt-in | Requires a NetworkPolicy-capable CNI and operator-owned ingress selectors / egress rules. |
| PodDisruptionBudget | `pdb.enabled=true` | Supported opt-in | Meaningful for multi-replica deployments only. Default is single replica and PDB off. |
| Non-root pod hardening | default `podSecurityContext`, `securityContext`, `automountServiceAccountToken=false` | Supported default | Chart lint asserts non-root, read-only root filesystem, dropped capabilities, seccomp, and no service-account token mount. |

### Not Supported As Chart-Owned Posture Today

- Public internet exposure directly from the Service without TLS/mTLS,
  mesh, gateway, or equivalent controls.
- Chart-managed Ingress. `values.yaml` has reserved ingress fields, but
  no Ingress template is shipped yet.
- Embedded NATS as a highly available durable event store. Current
  Choreographer events are fire-and-forget and the embedded profile
  disables JetStream by default.
- Multi-replica production state without an explicit state plan.
  Postgres covers current persistent repositories, but output-contract
  registration is still process-local and should be registered/seeded
  consistently after rollout.
- Provider egress allow-lists generated automatically by the chart.
  Add provider endpoint rules through `networkPolicy.egress.extra` or
  route providers through an operator-managed egress proxy.

### Change Rule

Changing Kubernetes posture support requires updating:

- `charts/choreographer/values.yaml`;
- any checked-in profile under `charts/choreographer/values.*.yaml`;
- relevant templates under `charts/choreographer/templates/`;
- `scripts/ci/helm-lint.sh`;
- `docs/operations/deploy-kubernetes.md`;
- this support matrix;
- `CHANGELOG.md`.
