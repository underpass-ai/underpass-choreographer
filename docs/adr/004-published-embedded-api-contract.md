# ADR-004: `choreo-api` is the contract a consumer compiles against

Status: Accepted

## Context

Products that embed the engine have been importing `choreo-embedded` and, with
it, the domain: `choreo-core` entities crossing into a consumer's dependency
graph, and every internal change of ours becoming a possible break of theirs.
One consumer's composition even re-exported our MCP server as its own surface.

The engine promises SemVer to its consumers, but there was no artifact whose
version that promise attached to. A consumer could not say "I compile against
contract 1" — only "I compile against whatever `choreo-embedded` was that week".

## Decision

`choreo-api` is a leaf crate holding the published contract of the embedded
engine, and nothing else:

- plain views (`CeremonySummary`, `CeremonyParticipant`) — projections a
  consumer can hold, log or map into its own vocabulary without importing the
  domain;
- a capability report (`ApiCapabilities`), stated by the implementation and
  checked by consumers at startup, because two builds of the same release can
  differ in features and a version string cannot say so;
- an error vocabulary (`ApiError`) that publishes whether each failure is worth
  retrying, so consumers do not keep their own staleness-prone tables;
- one trait (`CeremonyEngineApi`), read-only.

`CONTRACT_VERSION` moves on meaning, not on release: adding a capability keeps
the version, changing what an existing field or method means raises it.

Contract v3 makes the canonical publication identity available during
definition analysis. A publishable `DefinitionAnalysisView` carries the same
digest that `PublishedDefinitionView` will return and a started ceremony will
bind to. A defective draft carries no digest. This identity is derived from the
validated executable definition, not from its YAML source bytes.

Mutations are deliberately absent from v1. Starting, advancing and publishing
have their own use cases, transactionality and audit inside the engine; a
consumer that needs them coordinates through the engine's own surfaces. The
contract grows by adding named capabilities, never by widening what an existing
one means.

The crate depends on no `choreo-*` crate. `choreo-embedded` depends on it and
implements the trait; the dependency arrow points from implementation to
contract, never back.

`crates/choreo-api/src` joins the vocabulary gate's guarded paths. It is the
most public artifact this repository produces; a consuming product's term
landing there would be the defect ADR-001 names, published.

## Consequences

- A consumer compiles against `choreo-api` plus an implementation crate, and is
  testable against a stub of the trait alone.
- Instants travel as unix milliseconds and digests as hex strings: the contract
  does not export our choice of time or hash types as a dependency.
- A consumer can bind an approval or publication intent to the canonical digest
  before asking the engine to publish, then verify the returned receipt exactly.
- An instance started from an unpublished draft has **no** digest rather than a
  placeholder — a digest is a claim that a published, immutable definition ran,
  and a draft run must not make it.
- Everything the engine does beyond this contract remains reachable through
  `choreo-embedded`'s own methods, unversioned and unpromised. Consumers that
  keep using those surfaces are choosing coupling, and the line between the two
  is now visible in their `Cargo.toml`.
