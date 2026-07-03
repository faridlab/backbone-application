# Background & Prior Art

> **Reader:** evaluator. **Mode:** explanation. What came before, what this borrows, what it
> rejects — and honest credit to the tools it learned from.

## Where this sits in the landscape

Backbone is a Rust take on a familiar idea: a **batteries-included application framework** with a
**codegen-first** workflow and a **composition-root** service shape. Each of those pillars has
strong prior art. This page names it, credits it, and says what Backbone kept and dropped.

## Prior art, credited

### Laravel / Rails — convention-driven, scaffold-heavy frameworks

The `metaphor make:*` commands ([`metaphor make --help`]) are explicitly "Laravel-style
scaffolding." What Backbone borrows: strong conventions, generators that write the boilerplate, a
CLI that knows the project layout. What it rejects: dynamic, reflection-heavy runtimes. Backbone
generates *static Rust* checked by the compiler — the scaffolding is a build-time convenience, not a
runtime framework.

### Clean Architecture / Hexagonal / DDD

The `src/` layout is a direct encoding of Clean Architecture's dependency rule, with DDD's building
blocks (entities, value objects, aggregates, repositories, bounded contexts) mapped 1:1 to folders —
see [architecture/layers.md](./architecture/layers.md) and [architecture/ddd.md](./architecture/ddd.md).
What Backbone adds over "just read the book": the boundaries are mechanically enforced (a `sqlx`
import in `domain/` is a lint failure, not a code-review comment) and the folder names *are* the
ubiquitous language.

### Spring Boot / NestJS — module composition

The "compose independent modules into one deployable" shape echoes Spring's starters and Nest's
modules. Backbone's `Module::builder().with_database(pool).build()?` pattern and router merge
(see [`src/main.rs`](../src/main.rs)) are the same idea in Rust, minus the DI container — wiring is
explicit and visible in one file.

### Nx / Turborepo / Bazel — workspace orchestration

The `metaphor` CLI is a meta-workspace orchestrator: `metaphor list`, `graph`, `test --affected
--base=main`, `build`, `compose generate`. The affected-graph and per-project caching ideas come
straight from the JS/monorepo tooling world. The twist: each project keeps its **own git history**
and its **own** `Cargo.toml`/`package.json` — Metaphor coordinates independent repos rather than
owning one monorepo (`metaphor --help`: "manages a workspace of standalone project repos").

### Prisma / jOOQ / sqlc — schema-as-source-of-truth codegen

The schema-YAML-generates-everything model (`schema/models/*.model.yaml` → entities, migrations,
CRUD) is the same conviction as Prisma's schema or sqlc's SQL-first generation: one authoritative
definition, generated typed code downstream. Backbone's addition is the *regen-safety contract* —
`// <<< CUSTOM` markers, the `user_owned:` manifest, and migration header markers — so hand-written
code survives regeneration deterministically. See [`CLAUDE.md` → Regen safety](../CLAUDE.md).

## Why existing tools weren't enough

| Need | Why the off-the-shelf option fell short | Backbone's move |
|---|---|---|
| Type-safe, fast runtime | Rails/Laravel/Nest runtimes are dynamic; errors surface in production | Generate static Rust; SQLx checks queries at compile time |
| Codegen that survives edits | Prisma/sqlc regenerate wholesale; hand edits get clobbered | Explicit regen-safety markers + `user_owned:` manifest |
| Many services, shared domain | Monorepo tools own one repo; polyrepo loses coordination | Meta-workspace over standalone repos, each with its own history |
| Enforced architecture | Clean Architecture is a convention teams drift from | Dependency rule enforced by crate boundaries + lint |

## What it deliberately rejects

- **Runtime reflection / dynamic dispatch as the default.** Composition is explicit and compiled.
- **A monolith you fork.** Framework crates are consumed by version, not vendored.
- **Hand-ordered migrations.** Ownership of ordering moves to the CLI across modules.
- **ORM-as-lifestyle.** SQLx is used for compile-checked queries, not an active-record object graph.

## Next

- The concrete stack and each choice's rationale → [Technology & the "why"](./technology.md).
- The decisions recorded as ADRs → [ADR index](./adr/README.md).
