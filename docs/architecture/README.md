# Architecture Docs — Backbone Skeleton

This folder is the **detailed orientation** for anyone (human or AI) writing code in a service built on this skeleton. The root [`CLAUDE.md`](../../CLAUDE.md) is a lean entry point; the rules live here.

**Read this first for any non-trivial work.** Skim [`overview.md`](./overview.md) and [`ai-guidelines.md`](./ai-guidelines.md) before touching code.

## Index

| Doc | What it covers |
|---|---|
| [`overview.md`](./overview.md) | What this service is, how it sits in a metaphor consumer workspace, the `backbone-*` crates it composes, golden-path commands. |
| [`layers.md`](./layers.md) | Clean Architecture layout. `src/` folder → layer mapping, dependency rule, HTTP request flow, what each layer MUST / MUST NOT import. |
| [`ddd.md`](./ddd.md) | DDD applied here: bounded contexts, entities vs value objects vs aggregates, repositories, application services, anti-corruption layer. |
| [`solid.md`](./solid.md) | The five SOLID principles with codebase-grounded "good vs violation" examples. |
| [`clean-code.md`](./clean-code.md) | Naming, function size, error handling (`thiserror` vs `anyhow`), comment policy, no speculative abstractions. |
| [`ai-guidelines.md`](./ai-guidelines.md) | Behavioural rules every AI assistant MUST follow on this codebase. Cross-links into the other docs. |

## Conventions in these docs

- **MUST / SHOULD / NEVER** — rule strength, in the same voice as the rest of this repo's CLAUDE.md files.
- Every concrete claim cites a file path so it's verifiable (and so the doc rots loudly if the code moves).
- These docs do **not** re-document upstream framework APIs — they point at the relevant `backbone-*` crate source or its README.
- Placeholders in *italicised* `{{mustache}}` form (e.g. `{{service_name}}`, `{{ServiceName}}`, `{{module_name}}`) mark substitution targets when scaffolding a new project from this skeleton. They do **not** appear in load-bearing Rust files — the runnable skeleton uses the literal name `backbone-app`.
