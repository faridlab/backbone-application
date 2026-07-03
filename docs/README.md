# Backbone Service Handbook

> The complete documentation set for the **Backbone backend-service skeleton** — an
> Axum + SQLx + Tokio binary that *composes* framework crates and domain modules into a
> runnable HTTP/gRPC server. This page is the map; each section names the one reader it
> serves and the mode it's written in.

Business logic never lives here — it lives in upstream `module` projects. This service
loads config, connects Postgres, runs migrations, composes each `{Domain}Module` into one
router, wires health + maintenance + audit, and listens. Everything below documents *that*
job and how to extend it without breaking it.

## Who are you? Start here.

| You are… | You want to… | Go to |
|---|---|---|
| **Evaluator** | Decide whether to adopt this | [Philosophy](./philosophy.md) → [Background](./background.md) → [Technology](./technology.md) |
| **App developer** | Install it and ship an integration | **[Developer guide](./developer-guide.md)** ← start at the quickstart |
| **Maintainer** | Extend the service safely | [Architecture](./architecture/README.md) → [Maintainer guide](./maintainer-guide.md) |
| **Contributor** | Open a correct PR | [Contributing](./contributing.md) |
| **Anyone** | Agree on what a word means | [Glossary](./glossary.md) |

## The full section map

Ordered by the arc a serious reader takes — *why it exists* → *how it's built* → *how to use it* → *how to change it*.

| # | Section | Reader | Diátaxis mode |
|---|---------|--------|---------------|
| 1 | [Philosophy & motivation](./philosophy.md) | Evaluator | Explanation |
| 2 | [Background & prior art](./background.md) | Evaluator | Explanation |
| 3 | [Technology & the "why"](./technology.md) | Evaluator + Maintainer | Explanation |
| 4 | [Architecture](./architecture/README.md) | Maintainer | Explanation |
| 5 | [Maintainer guide](./maintainer-guide.md) | Maintainer | How-to |
| 6 | [App-developer guide](./developer-guide.md) | App developer | Tutorial + How-to |
| 7 | [Contribution guide](./contributing.md) | Contributor | How-to |
| 8 | [Glossary / ubiquitous language](./glossary.md) | All | Reference |
| 9 | [Architecture Decision Records](./adr/README.md) | Maintainer | Explanation |

The **Architecture** section (4) already ships as a detailed folder:
[`overview`](./architecture/overview.md), [`layers`](./architecture/layers.md),
[`ddd`](./architecture/ddd.md), [`solid`](./architecture/solid.md),
[`clean-code`](./architecture/clean-code.md), [`ai-guidelines`](./architecture/ai-guidelines.md).
This handbook wraps it with the evaluator, app-developer, and contributor sections it didn't yet have.

## How this handbook relates to the other docs

- Root [`CLAUDE.md`](../CLAUDE.md) — the lean, always-loaded entry point for AI assistants. Rules
  live in [`docs/architecture/`](./architecture/README.md); this handbook is the human-facing set.
- [`README.md`](../README.md) — the 60-second "clone and run" blurb. This handbook is where you go
  when 60 seconds isn't enough.
- The workspace [`metaphor.yaml`](../../metaphor.yaml) — the authoritative inventory of sibling
  projects (framework, modules, CLI, mobile app) this service composes with.

## Ground truth

Every concrete claim in this handbook cites a file so it rots loudly when the code moves. When a
doc and the code disagree, **the code wins** — fix the doc and flag the drift. Sources of truth:

- [`Cargo.toml`](../Cargo.toml) — which crates and which tag (`v2.1.0`) are composed.
- [`src/main.rs`](../src/main.rs) — the exact bootstrap order.
- [`config/application.yml`](../config/application.yml) (+ `-dev` / `-prod` overlays) — config shape.
- [`metaphor.codegen.yaml`](../metaphor.codegen.yaml) — what the generator must never touch.
- The live CLI: `metaphor <cmd> --help` (this handbook was written against `metaphor 0.2.0`).

*Last reconciled against the tree on 2026-07-03.*
