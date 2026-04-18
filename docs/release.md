# Release process

A release cuts a versioned container image + a Helm chart to
`ghcr.io/underpass-ai/*`. Both are driven by
`.github/workflows/publish-distribution.yml`, which triggers on any
`v*` tag pushed to the repository.

## Versioning

Semver. Two places must stay in lockstep:

- `Cargo.toml` → `[workspace.package].version`
- `charts/choreographer/Chart.yaml` → `version` + `appVersion`

`scripts/release.sh version <X.Y.Z>` (or `just version 0.2.0`)
rewrites both in one pass and is idempotent.

## Checklist

Run through this before tagging. Each item has a `just` recipe that
mirrors the CI gate.

1. **Sync main** — you must release off main:
   ```bash
   git checkout main && git pull --ff-only
   ```
2. **Bump versions** (see script above):
   ```bash
   just version 0.2.0
   git diff                 # review
   ```
3. **Fast gates** green locally:
   ```bash
   just check               # fmt-check + clippy + test + bench-compile
   just helm-lint           # chart hardening assertions
   ```
4. **Container-backed gates** green locally:
   ```bash
   just integration         # integration-nats + integration-postgres
   ```
5. **End-to-end** (skipped on per-PR CI, run here):
   ```bash
   just e2e-compose         # full stack
   just e2e-kubernetes      # kind + chart + runner
   ```
6. **Commit the version bump** and open a PR:
   ```bash
   git commit -am "chore: v0.2.0"
   gh pr create --fill
   # wait for CI green; merge
   ```
7. **Tag and push** (only from merged main):
   ```bash
   git checkout main && git pull --ff-only
   just release 0.2.0
   ```
8. **Verify the publish-distribution workflow succeeded**:
   ```bash
   gh run watch $(gh run list --workflow publish-distribution.yml --json databaseId -q '.[0].databaseId')
   ```

After step 8, both artefacts are live:

- `ghcr.io/underpass-ai/underpass-choreographer:v0.2.0`
- `oci://ghcr.io/underpass-ai/charts/choreographer:0.2.0`

## What `just release` does

1. Asserts the working tree is clean.
2. Asserts `Cargo.toml` and `Chart.yaml` already reflect the
   target version (the bump must have happened + merged first).
3. Asserts the current branch is `main`.
4. Asserts the tag does not exist yet.
5. Creates an annotated `vX.Y.Z` tag at HEAD and pushes it.

The script does **not** push the tag without every gate passing —
the actual gates are your local `just check && just integration &&
just e2e-compose` (step 5 of the checklist). Automating them on
tag push would delay the signal; running them beforehand is fast
and deterministic.

## Hotfix flow

Same checklist, from a hotfix branch off the tag you want to fix:

```bash
git checkout -b hotfix/v0.2.1 v0.2.0
# ... fix ...
just version 0.2.1
git commit -am "chore: v0.2.1"
gh pr create --fill
# merge; then from main:
just release 0.2.1
```

## Rolling back

If a published image is bad, do **not** delete the tag — immutable
releases are an invariant. Cut a new patch version that reverts or
fixes the commit and re-release.
