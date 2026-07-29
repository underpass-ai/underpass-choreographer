#!/usr/bin/env bash
set -euo pipefail

# ADR-001: no consuming product's vocabulary enters this repository.
#
# The engine coordinates working sessions for any domain. A term from one
# vertical reaching the core, the public tool surface or a published
# artifact is a defect, not a naming preference — it narrows an engine
# that is only worth publishing separately because it stays general.
#
# The authoring surface is where foreign vocabulary is most likely to
# enter, which is why this is a gate rather than a review note.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

# Terms from specific operational verticals. Extend deliberately: every
# entry must be a word the generic engine has no reason to know.
VERTICAL_TERMS='incident|outage|postmortem|on-call|oncall|pagerduty|sev[0-9]|runbook'

GUARDED_PATHS=(
  'crates/choreo-core/src'
  'crates/choreo-app/src'
  'crates/choreo-mcp/src'
)

failed=0

for path in "${GUARDED_PATHS[@]}"; do
  if [[ ! -d "${path}" ]]; then
    echo "domain-vocabulary-boundary: guarded path is missing: ${path}" >&2
    exit 1
  fi

  if matches="$(grep -rniE "${VERTICAL_TERMS}" "${path}")"; then
    echo "domain-vocabulary-boundary: vertical vocabulary found in ${path}" >&2
    echo "${matches}" >&2
    failed=1
  fi
done

if (( failed )); then
  cat >&2 <<'EOF'

A consuming product names its own concepts in its own repository and maps
them at its boundary, the same way it already supplies evidence sources
and context bundles. See docs/adr/001-working-session-vocabulary.md.
EOF
  exit 1
fi

echo "domain-vocabulary-boundary: clean"
