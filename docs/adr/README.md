# Architecture decision records

Durable decisions about the Choreographer core, its contracts and its
distributions. Each record states what was decided and what it costs, not how
the code is organised — that lives in the architecture docs.

A decision is superseded, never rewritten: the reason it changed is worth as
much as the decision itself.

- [ADR-001](001-working-session-vocabulary.md): working sessions are the public
  name; `Ceremony` is the domain
- [ADR-002](002-ceremony-definition-analysis.md): analysis reports every defect;
  construction still fails fast
- [ADR-003](003-audit-journal-and-durability.md): the engine owns the audit
  contract; the host owns durability
- [ADR-004](004-published-embedded-api-contract.md): `choreo-api` is the
  contract a consumer compiles against
