# Philosophy & Motivation

> **Reader:** evaluator — you're deciding whether to build on this. **Mode:** explanation.
> No commands here; this is the worldview that explains every trade-off in the rest of the handbook.

## The problem

Most backend services rot the same way. Business rules, HTTP plumbing, SQL, and infrastructure
config congeal into one codebase where changing an invoice rule means editing a router, and the
same "user" entity gets redefined slightly differently in three services. Framework upgrades
become archaeology. Nobody can say where a rule *lives*.

Backbone's answer is a hard split: **a service composes; a module implements.** This repository is
a *service skeleton* — a runnable Axum binary whose entire job is to assemble framework crates and
domain modules into one server. It owns no business logic. If you find yourself writing an entity
invariant in this repo, you're in the wrong repo.

## The worldview

Three convictions drive the design.

1. **Composition over implementation.** A deployable service is a thin composition root. It loads
   config, opens a pool, runs migrations, merges each module's router, and listens. That's the
   whole of [`src/main.rs`](../src/main.rs). Everything else is imported.

2. **A single source of truth generates the rest.** In a domain `module`, the schema YAML
   (`schema/models/*.model.yaml`) is authoritative; entities, CRUD wiring, migrations, and DTOs are
   *generated* from it. Hand-written code is the exception, and it announces itself
   (`// <<< CUSTOM` markers, `user_owned:` manifest entries). This is what keeps a codebase the
   shape it was designed to have across hundreds of AI-assisted and human edits.

3. **Layers are load-bearing, not decorative.** Clean Architecture's dependency rule is enforced,
   not aspirational: `domain/` may not import `sqlx` or `axum`; a leak is a bug, not a style nit.
   See [architecture/layers.md](./architecture/layers.md).

## What it refuses to do (non-goals)

Trust comes from honest limits. This skeleton deliberately does **not**:

- **Hold business logic.** No entities, no domain services, no feature endpoints ship here — only
  `/health` and a maintenance gate. Features live in `module` projects.
- **Be a framework you fork.** You compose the `backbone-*` crates by version; you don't vendor and
  edit them. Upstream fixes come back via a tag bump, not a local patch.
- **Support hand-ordered SQL migrations.** The bundled `migrate` subcommand is a deliberate no-op
  stub; ordering is owned by `metaphor migration run-all` across modules.
- **Be a workspace.** The standalone repo is a single app meant to be cloned, renamed, and dropped
  into a metaphor consumer workspace under `apps/`. It doesn't orchestrate siblings.
- **Chase feature breadth in the transport layer.** gRPC and GraphQL are feature-gated and optional;
  the default surface is REST over Axum.

## Why a skeleton at all?

Because the alternative — every team scaffolding bootstrap-by-hand — reproduces the same subtle
mistakes (mis-ordered middleware, secrets in code, ad-hoc migration running) once per service. The
skeleton encodes the correct bootstrap *once*: subcommand dispatch before observability init so
`healthcheck` stays cheap, pool prewarm before first request, maintenance gate outermost, audit
innermost. Read the ordering comments in [`src/main.rs`](../src/main.rs) — each one is a lesson paid
for elsewhere.

## The test of success

You've adopted this correctly when:

- Adding a feature never touches this repo's `domain/` — it's a module change plus a version bump.
- A framework security fix reaches every service by bumping one tag.
- A new engineer ships their first integration from the [developer guide](./developer-guide.md) in
  an afternoon, without reading a line of architecture theory.

If a change to this service starts feeling like "real programming," stop — the feature belongs
upstream. That discomfort is the design working.

## Next

- What came before and why it fell short → [Background & prior art](./background.md).
- The stack and the reasoning behind each choice → [Technology & the "why"](./technology.md).
