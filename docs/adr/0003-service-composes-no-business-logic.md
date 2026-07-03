# ADR-0003: The service composes; it never implements business logic

- **Status:** Accepted
- **Deciders:** Backbone maintainers
- **Applies to:** this `backend-service` skeleton and every service rendered from it

## Context

A deployable service is the natural place for feature code to accumulate — it's what runs, it's what
gets deployed, it's where the endpoint "obviously" goes. But if business logic lives in the service,
several problems follow:

- The same domain concept gets re-implemented, slightly differently, in every service that needs it.
- A framework or domain fix has to be applied service-by-service.
- Nobody can point to where a rule *lives*; it's smeared across routers and handlers.
- The service can't be regenerated or restructured without risking hand-written business code.

## Decision

**This service is a composition root only.** Its entire job, encoded in [`src/main.rs`](../../src/main.rs),
is: load config → open the pool → run migrations → compose each `{Domain}Module` into one router →
wire health/maintenance/audit → listen. Business logic — entities, invariants, domain rules, feature
endpoints — lives exclusively in `module` projects upstream and is pulled in by version. The skeleton
ships **no** business endpoints, only `/health` and a maintenance gate.

## Consequences

**Positive**

- One home per domain concept (the owning module); no cross-service duplication.
- A framework/domain fix reaches every service via a single tag bump.
- The service stays thin, regenerable, and easy to reason about — the bootstrap is one readable file.
- Clear rule for contributors and AI assistants: business logic here is a bug.

**Negative / costs**

- A change that "feels" like it belongs in the service (a small orchestration, a one-off endpoint)
  often has to go upstream — more ceremony for small features.
- Requires discipline and enforcement; the boundary is easy to erode one "just this once" at a time.
- Cross-context orchestration that legitimately belongs at the composition layer needs a documented
  home (`src/application/services/`, hand-written) so it isn't mistaken for a boundary violation.

## Enforcement

Stated as a hard rule in [root `CLAUDE.md`](../../CLAUDE.md) and
[architecture/ai-guidelines.md §5–6](../architecture/ai-guidelines.md); checked in review per the
[contribution guide](../contributing.md#scope-discipline).
