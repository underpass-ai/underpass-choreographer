# ADR-003: The engine owns the audit contract; the host owns durability

Status: Accepted

## Context

Publishing a typed event does not make an audit. A message can be lost,
duplicated, published after persistence failed, or stored without a verifiable
sequence. Metrics, traces and logs give operational diagnosis, not a canonical
history.

The engine also has mechanisms that exist specifically to survive failure —
step leases for failover, idempotency keys so a retry does not duplicate
effects, human guards that block until a person decides. All three currently
live in memory, including in the deployable server, which wires ceremonies to an
in-memory repository. A guard that disappears on restart is not a guard; a lease
that does not outlive the failure it exists for is decorative.

## Decision

**The engine defines and verifies the contract. The host provides the storage.**
`AuditRecord`, the hash chain, the event catalogue and `CeremonyUnitOfWorkPort`
belong to the engine. Which durable store backs them is the host's choice, the
same way context materialization already is.

**A conformance suite is part of the contract, not an extra.** An adapter that
does not pass it does not implement the port. Without it, "the host implements
persistence" would mean nobody guarantees anything, and the engine could not
claim auditability at all.

**Tamper evidence stays entirely in the engine.** `record_hash = hash(canonical
payload + previous_record_hash)`, monotonic sequence per `ceremony_id`,
independent verifier. None of it depends on the storage engine, so none of it is
delegated.

**One transaction covers snapshot, journal and outbox.** Saving state,
appending records and enqueueing messages either all happen or none do. Delivery
is asynchronous and at-least-once with idempotent consumers.

**The outbox is a table beside the state, not a broker feature.** With an outbox
that marks a row delivered only after the publish succeeds, at-least-once holds
regardless of broker durability. The NATS adapter is plain core pub/sub and says
so; **JetStream is therefore not required**, and no new infrastructure
dependency enters for auditability.

**The embedded reference implementation uses redb.** It has no non-optional
dependencies and no C toolchain, so the embedded distribution stays free of
system dependencies. One write transaction spans several tables, which is the
unit of work directly. A composite `(ceremony_id, sequence)` key makes the
monotonic sequence and its uniqueness inherent rather than a declared
constraint, and gives range scans for chain verification.

**The server distribution uses PostgreSQL.** redb takes an exclusive file lock
and serves a single process, which is right for embedded and wrong for
replicas. Both adapters implement one port and pass one conformance suite.

**Snapshot plus append-only journal plus outbox, not event sourcing.** It gives
strong auditability at materially lower risk, and does not foreclose the move.

**Decision reconstruction, not deterministic replay.** Exact replay cannot be
promised over non-deterministic components. What is preserved is enough to
reconstruct why a decision was made: provider, model, parameters, prompt
template version and digest, tool catalogue digest, context, evidence, tool
results, human guards, output contract, exact definition, adapter versions.

## Consequences

A crash boundary cannot be tested against an in-memory adapter, so the engine
needs at least one durable implementation of its own — not to be authoritative,
but to demonstrate the contract is satisfiable.

Two independent implementations of the same port, passing the same suite, are
better evidence than one: they show the port is not shaped around a single
store.

The deployable server stops losing ceremonies on restart, which is a precondition
for the ceremony engine being a product rather than a demo.

The engine's public claim becomes precise: it defines, chains and verifies the
audit contract; durability is the host's, and conformance is how that stays
honest.
