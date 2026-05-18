# Deploying the Choreographer to Kubernetes

The chart at `charts/choreographer/` ships with a deployment profile
for the `underpass-runtime` namespace
(`charts/choreographer/values.underpass-runtime.yaml`) and a wrapper
script (`scripts/ci/deploy-kubernetes.sh`).

The profile reflects the agreed Underpass topology: every plane
(KMP, Runtime, Choreographer) owns its **own NATS bus** — the planes
don't share subjects and there is no cross-plane NATS subscriber, so
collocating buses would couple deploys without sharing data. The
choreographer's chart therefore deploys a release-local NATS via
`messaging.nats.embedded.enabled: true`.

## Prerequisites

1. **Image pull secret.** A namespace secret `ghcr-pull`
   (`kubernetes.io/dockerconfigjson`) with credentials that can pull
   from `ghcr.io/underpass-ai/`.
2. **Runtime mTLS client cert** (only when `executor.kind: runtime`).
   Apply
   [`tests/cluster/choreographer-runtime-client-cert.yaml`](../../tests/cluster/choreographer-runtime-client-cert.yaml)
   first. It mints a `kubernetes.io/tls` secret named
   `choreographer-runtime-client-tls` via cert-manager, signed by the
   same CA that signs `underpass-runtime`'s server cert.
3. **Reachable runtime** — `underpass-runtime` Service must exist in
   the namespace and the chart values (or the override) must point at
   it (`executor.runtime.endpoint: https://underpass-runtime:50053`).

## Deploy

```bash
NAMESPACE=underpass-runtime \
RELEASE_NAME=choreographer \
IMAGE_TAG=sha-<commit> \
VALUES_FILE=charts/choreographer/values.underpass-runtime.yaml \
./scripts/ci/deploy-kubernetes.sh
```

The wrapper:

- Requires `IMAGE_TAG` **or** `IMAGE_DIGEST` (mutually exclusive).
- Defaults to `--wait --atomic --timeout 10m`. `DRY_RUN=true` falls
  back to `--dry-run=server`.
- Always passes `--create-namespace`.

## Smoke

```bash
# Liveness + readiness via the HTTP sidecar port (NATS readiness
# is part of /readyz).
kubectl -n "$NAMESPACE" port-forward svc/choreographer 8080:8080 &
curl -s localhost:8080/healthz
curl -s localhost:8080/readyz   # {"checks":[{"name":"nats","healthy":true,...}]}
```

## End-to-end against the deploy

`tests/cluster/e2e-job.yaml` runs the `choreo-e2e-runner` binary as
a Job. The current runner is the same nine-scenario binary used by
compose. Scenarios 1–4 cover gRPC + NATS + causal metadata against a
real deploy and pass when the choreographer is wired correctly.

Scenarios 5-9 are compose-shaped: scenario 5 expects a Runtime tool
named `stub.echo`, scenarios 8-9 expect the `stub-llm` OpenAI-
compatible sidecar, and scenario 6 asserts strict rejection of the
NoopAgent's free-form output. Against the real `underpass-runtime`,
the `stub.echo` tool is not normally in the catalog, so the runner can
exit with `NotFound: runtime resource` from scenario 5. That proves
the Runtime gRPC path was reached, but it is not a green full-stack
acceptance signal. Treat the cluster Job as a targeted connectivity
smoke unless the namespace also provides the same stub services or
equivalent test fixtures.

```bash
kubectl apply -f tests/cluster/e2e-job.yaml
kubectl -n "$NAMESPACE" logs -f -l app.kubernetes.io/component=e2e
```

## Rolling back

Helm-managed:

```bash
helm -n "$NAMESPACE" rollback choreographer <revision>
```

The script's `--atomic` ensures a failed `helm upgrade` already
rolls back the release before it returns non-zero.

## Topology recap

```
+---------------------------------------------+
|                underpass-runtime ns         |
|                                             |
|   choreographer  <----- gRPC mTLS ---->  underpass-runtime
|   |                                          |
|   | NATS                                    | NATS
|   v                                          v
|   choreographer-nats                  underpass-runtime-nats
|                                             |
|   rehydration-kernel  <-- gRPC -->  rehydration-kernel-nats
|                                             |
+---------------------------------------------+
```

Each plane owns its NATS. No subject collisions exist across the
three planes; integration is gRPC (point-to-point) where it matters.
