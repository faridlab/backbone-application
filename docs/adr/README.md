# Architecture Decision Records

> **Reader:** maintainer. **Mode:** explanation. One record per decision: context, decision, status,
> consequences. **ADRs are immutable once accepted — supersede, don't edit.**

An ADR captures *why* a load-bearing decision was made, so a future maintainer doesn't re-litigate
it from scratch or reverse it without knowing what it was protecting.

## When to write one

Write an ADR when a change alters an architectural boundary, a public interface, a technology
choice, or a convention every other contributor must follow. Don't write one for a bug fix or a
routine dependency bump. If you're tempted to bury a decision in a code comment — write the ADR instead.

## Index

| ID | Title | Status |
|---|---|---|
| [0001](./0001-subprocess-dispatched-plugins.md) | Subprocess-dispatched plugins | Accepted |
| [0002](./0002-schema-yaml-single-source-of-truth.md) | Schema YAML as the single source of truth | Accepted |
| [0003](./0003-service-composes-no-business-logic.md) | The service composes; it never implements business logic | Accepted |

## Writing a new ADR

1. Copy an existing record as a template (or the skeleton at
   `.claude/skills/framework-handbook/templates/adr-NNNN.md`).
2. Number it sequentially (`NNNN`), give it a short imperative title.
3. Fill **Context → Decision → Status → Consequences**. Be honest about the downsides in Consequences.
4. Add the row to the index above.
5. To reverse a decision, add a **new** ADR that supersedes the old one and flip the old one's status
   to `Superseded by NNNN`. Never rewrite an accepted record.

## Status values

`Proposed` → `Accepted` → (`Superseded by NNNN` | `Deprecated`). A `Rejected` record is still worth
keeping — it documents a path considered and declined.
