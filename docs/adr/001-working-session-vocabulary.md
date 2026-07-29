# ADR-001: Working sessions are the public name; `Ceremony` is the domain

Status: Accepted

## Context

The engine coordinates structured, governed sessions between agents and humans.
"Ceremony" is precise inside the domain — a formal, versioned, repeatable,
governed process — and opaque as a public term. Downstream products that embed
the engine carry their own vocabulary for the work they coordinate.

## Decision

Four layers, and only four:

| Layer | Term |
| --- | --- |
| Domain types in `choreo-core` | `Ceremony` |
| Public English | working session |
| Public Spanish | mesa de trabajo |
| Consuming product | its own, never here |

`choreo-core` must not gain a second, parallel hierarchy for the same concept.
There is no `WorktableDefinition` next to `CeremonyDefinition`.

No vocabulary from a consuming product enters this repository — not in domain
types, not in tool names, not in commit messages. A product-specific term
reaching a public artifact of this repo is a defect, not a naming preference.

Ceremony patterns are data. They are never types in `choreo-core`, because the
first patterns anyone writes will be shaped by whatever domain they came from.

## Consequences

A product embeds the engine by naming its own concepts in its own repository and
mapping them at its boundary, the same way it already supplies evidence sources
and context bundles.

The engine stays usable by any coordination domain, which is the condition for
it being worth publishing separately at all.

This constraint is worth enforcing mechanically rather than by review, since the
authoring surface is where foreign vocabulary is most likely to enter.
