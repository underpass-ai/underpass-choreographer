# ADR-002: Analysis reports every defect; construction still fails fast

Status: Accepted

## Context

`CeremonyDefinition::new` validated fail-fast, returning the first
`DomainError` it met. That is enough to reject a definition and not enough to
correct one.

The engine's differentiation is that a definition can be designed at runtime —
by an agent, during the session it is about to govern — and still be safe,
because publication passes a deterministic validator. An author that receives
one error per attempt cannot converge on a valid definition; an author that
receives the whole set can.

## Decision

**Analysis accumulates; `new` keeps its behaviour.** `analyze` collects findings
in check order and `validate` returns the first blocking one, so construction
raises exactly the error it raised before, for the same input.

**A finding carries a locus.** A typed `DomainError` says what is wrong; the
locus says which state, transition, step, guard or role. Both are needed to
correct a draft without guessing.

**A new criterion enters as a warning, never as an error.** Promoting a warning
rejects definitions that construct today, so it is a separate decision taken
with evidence — starting with whether the shipped catalog trips it. Reachability
findings landed this way.

**Analysis is skipped where it would only add noise.** Reachability is not
reported when the graph is too broken to walk; structural errors come first.

**`CeremonyDefinitionDraft` carries a definition under authoring.** A
`CeremonyDefinition` is always valid, which is the right guarantee for execution
and the wrong one for authoring: the definition an author most needs feedback
about is the one that does not construct. Duplicate declarations are findings,
not aborts.

**There is exactly one place where the invariants are enforced.**
`Draft::publish` delegates to `CeremonyDefinition::new`. The draft neither wraps
nor reimplements it, and no unchecked constructor exists.

**Parse failures and structural defects are different kinds.** A malformed
identifier is a parse failure; anything structural is always a finding, never an
exception.

**The report is not serializable.** It carries a `DomainError`, and
serialization belongs to the adapter layer. Adapters render their own wire
shape.

**Authoring tools are read-only and stateless.** `choreo_validate_ceremony_draft`
and `choreo_explain_ceremony_draft` neither publish nor execute, so they hold no
state and never reach the choreographer. They ship on the `embedded` feature:
`choreo-core` and `choreo-adapters` are optional dependencies of a published
crate, and serving the gRPC backend as well is a proto change, not a flag.

## Consequences

The existing test suite is the proof that the accumulating refactor is faithful,
because no check changed.

Report and publication cannot diverge silently: publishing fails with exactly
the first blocking finding, and a test pins that.

Publication itself is deliberately not part of this decision. Publishing is an
audited mutation whose digest every audit record references, so it belongs after
the audit journal — see [ADR-003](003-audit-journal-and-durability.md).
