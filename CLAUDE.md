# Metaphor Backend Service

> Type: **`backend-service`** — a runnable Rust HTTP/gRPC server. Composes framework crates + domain modules. Has a `main.rs`.
> This file orients Claude. Skills carry depth; load them on demand.

## What this is

An Axum + SQLx + Tokio binary that bootstraps the server: loads config, runs migrations, composes `{Domain}Module` libraries into a single router, wires health checks, starts listening. Business logic lives in `module` projects — this service only composes.

## Golden path

```bash
metaphor dev serve                        # REST + gRPC on default ports
metaphor dev serve --rest-only --port 8080
metaphor dev db migrate                   # apply module migrations
metaphor dev test --integration-only      # end-to-end tests
metaphor lint check                       # clippy + fmt + audit
```

## Rules

- **MUST** have a `main.rs` and `[[bin]]` target.
- **MUST** load config from `config/application*.yml` via the framework config loader; no hardcoded secrets.
- **MUST** register every domain module in `main.rs` via its builder (`AccountingModule::builder().with_database(pool).build()?`) and merge its router.
- **MUST** run migrations through `backbone_orm::migrations::MigrationManager` (or `metaphor dev db migrate`) — never hand-ordered `sqlx::migrate!`.
- **MUST** use `BackboneCrudHandler` / `GenericCrudService` / `GenericCrudRepository` wiring; don't hand-roll CRUD routes.
- **NEVER** put business logic here. Feature work belongs in the owning `module` project.
- **NEVER** hand-write SQL migrations when the module's schema YAML can regenerate them. Edit `schema/models/*.model.yaml` upstream instead.
- **SHOULD** expose `/health`, `/readyz`, `/metrics` (Prometheus) and structured JSON logs.
- **SHOULD** feature-gate optional transports (`grpc`, `graphql`) when the module supports them.

## Folder cheatsheet

```
src/
├── main.rs                # bootstrap: config → pool → migrations → modules → router → listen
├── config.rs              # thin wrapper over framework config loader
├── bootstrap/             # module composition helpers
│   └── mod.rs
└── routes.rs              # top-level router merge (health + module routes)

config/
├── application.yml        # base
├── application-dev.yml    # APP_ENV=dev overlay
└── application-prod.yml   # APP_ENV=prod overlay

migrations/                # runtime-applied (aggregates module migrations)

Cargo.toml                 # deps: backbone-* crates + domain modules
```

## Tech stack (non-negotiable)

- HTTP: **Axum 0.7**
- gRPC: **Tonic 0.12** + Prost 0.13 (feature-gated)
- DB: **PostgreSQL** via **SQLx 0.8** (compile-time checked queries)
- Async: **Tokio 1.x** (full features)
- Serialization: serde / serde_json / serde_yaml
- Errors: `thiserror` for typed boundaries, `anyhow` inside `main.rs`
- Observability: `tracing` + `tracing-subscriber` (JSON in prod), Prometheus metrics via `backbone-metrics`

## Common tasks

- "Add a new domain endpoint" → go to the owning `module` project, edit schema YAML there, regenerate, then here just register the module in `main.rs` if not already.
- "Bump a module version" → update dep in `Cargo.toml`, `metaphor dev db migrate`, `metaphor dev test`.
- "Run locally with docker deps" → `metaphor dev serve --docker` (spins up postgres/redis via compose).
- "Add rate limiting / auth middleware" → wire in the framework's middleware tower layer; don't write it from scratch.

## Key files to read before editing

- `src/main.rs` — bootstrap sequence; know what runs in what order.
- `Cargo.toml` — which modules are composed.
- `config/application.yml` — shape of config; env overlays.
- Each imported module's `CLAUDE.md` — its rules apply to features it owns.

## Deeper knowledge (load on demand)

- Skill: `backbone-cli-master` — Backbone CLI surface + workflows.
- Skill: `backbone-modules-orchestrator` — composing modules into a service.
- Skill: `backbone-framework-architect` — framework crate layering.
- Skill: `api-and-interface-design` — REST/gRPC/GraphQL surface shape.
- Skill: `security-and-hardening` — authz, input validation, secret handling.

## Anti-patterns

- Writing business logic in `main.rs` (belongs in a module).
- Hand-rolled `axum::Router` routes for CRUD (use `BackboneCrudHandler`).
- Hardcoded database URLs / secrets (use config + env).
- Skipping `MigrationManager` and running `sqlx migrate` ad-hoc.
- Blocking code inside Tokio tasks (`std::fs::read`, `reqwest::blocking`) — use async equivalents.
