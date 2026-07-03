# Technology & the "Why"

> **Reader:** evaluator + maintainer. **Mode:** explanation. Every choice gets a one-line rationale
> and a named alternative that was rejected. This is where the ADRs get summarized.

The stack is non-negotiable for a reason: consistency across services is worth more than
per-service optimization. If you're weighing a swap, read the rejected alternative first.

## Runtime & language

| Choice | Rationale | Rejected alternative |
|---|---|---|
| **Rust (edition 2021)** | Compile-time guarantees + no GC pauses; the codegen emits static, checked code | Go (simpler, but weaker type system for the DDD encoding); Kotlin/JVM (used for the mobile app, not the service) |
| **Tokio 1.x** (full features) | The de-facto async runtime; every `backbone-*` crate targets it | async-std (smaller ecosystem, losing momentum) |

> **Never block the Tokio runtime.** No `std::fs::read`, no `reqwest::blocking` inside tasks — use
> async equivalents. This is an enforced anti-pattern, see [`CLAUDE.md`](../CLAUDE.md).

## HTTP & transports

| Choice | Rationale | Rejected alternative |
|---|---|---|
| **Axum 0.7** | Tower-based, composes middleware as layers, first-class in the ecosystem | actix-web (its own actor model complicates shared middleware); warp (filter model harder to read) |
| **Tonic 0.12 + Prost 0.13** (feature-gated `grpc`) | Standard Rust gRPC; only compiled when a module needs it | manual protobuf handling |
| **GraphQL** (feature-gated `graphql`) | Optional transport for modules that expose it | making it default (most services don't need it) |

REST over Axum is the default surface. gRPC and GraphQL are **feature-gated** so a service pays for
them only when a module opts in.

## Data & persistence

| Choice | Rationale | Rejected alternative |
|---|---|---|
| **PostgreSQL** | Transactions, JSON, mature ops story; the one DB the framework targets | MySQL (weaker JSON/DDL story); a NoSQL default (loses relational invariants the domain needs) |
| **SQLx 0.8** (compile-time checked queries) | Catches broken SQL at build time, no ORM object graph | Diesel (heavier macro/DSL); SeaORM (active-record style Backbone deliberately avoids) |
| **Schema YAML → generated migrations** | One source of truth; ordering owned by `metaphor migration run-all` | hand-written `sqlx::migrate!` (drifts, mis-orders across modules) |

The `migrate` subcommand in [`src/main.rs`](../src/main.rs) is a **placeholder stub** on purpose —
migration ordering across modules belongs to the CLI, not to any one service binary.

## Configuration

| Choice | Rationale | Rejected alternative |
|---|---|---|
| **YAML + `${VAR:default}` env overlay** | Human-readable base, per-env overlays, env override for secrets | env-only (unreadable for nested config); hardcoded (secrets in code — forbidden) |
| **`config`/`serde_yaml` loader** | Standard, typed deserialization into `AppConfig` | bespoke parser |

Layering: [`config/application.yml`](../config/application.yml) → `application-${APP_ENV}.yml`
overlay → env-var override. Dev-default secrets trigger a **boot-time warning**
(`validate_defaults`) so `change-me-in-production` never ships silently.

## Errors

| Choice | Rationale | Rejected alternative |
|---|---|---|
| **`thiserror`** at typed boundaries | Explicit, matchable error enums between layers | stringly-typed errors |
| **`anyhow`** *inside `main.rs` only* | Bootstrap is allowed ergonomic `?` on heterogeneous errors | `anyhow` everywhere (erases the typed boundaries the layers depend on) |

The split is a rule: `anyhow` is confined to `main.rs`; every other layer uses typed `thiserror`
errors. See [architecture/clean-code.md](./architecture/clean-code.md).

## Observability

| Choice | Rationale | Rejected alternative |
|---|---|---|
| **`tracing` + `tracing-subscriber`** | Structured, async-aware spans; JSON in prod, pretty in dev | `log` (no span/context model) |
| **Prometheus via `backbone-observability`** | Standard metrics scrape; feature-gated | bespoke metrics endpoint |
| **Audit middleware** (innermost layer) | Records the actual response status the client sees | logging at the edge (misses post-middleware status) |

## The `backbone-*` crate suite

The service composes 16 framework crates at tag `v2.1.0` ([`Cargo.toml`](../Cargo.toml)): core CRUD
(`backbone-core`, `backbone-orm`), web stack (`backbone-auth`, `backbone-authorization`,
`backbone-rate-limit`), ops (`backbone-health`, `backbone-maintenance`, `backbone-observability`),
messaging (`backbone-messaging`, `backbone-queue`, `backbone-jobs`), storage (`backbone-storage`,
`backbone-cache`, `backbone-search`), and comms (`backbone-email`, `backbone-graphql`). See
[architecture/overview.md](./architecture/overview.md) for what each group provides.

**Why a suite of small crates, not one mega-crate?** A service compiles and links only what it
composes; feature flags gate optional transports; a security fix to one concern is one crate bump.
The trade-off — many crates to version — is managed by pinning them all to a single tag.

## The `metaphor` CLI & subprocess plugins

The tooling is a meta-CLI (`metaphor 0.2.0`) that orchestrates independent project repos. Its
generators and dev commands live in **separate plugin binaries** dispatched as subprocesses
(`metaphor-codegen`, `metaphor-schema`, `metaphor-dev`, `metaphor-agent`). **Why subprocess, not
in-process?** A plugin can crash, be upgraded, or be swapped without rebuilding the core CLI, and
each plugin owns its own dependency tree. This is recorded in [ADR-0001](./adr/0001-subprocess-dispatched-plugins.md).

## Next

- How the pieces fit at runtime → [Architecture](./architecture/README.md).
- The decisions as immutable records → [ADR index](./adr/README.md).
