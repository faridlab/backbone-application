# Backbone Backend Service Skeleton

> Type: **`backend-service`** — a runnable Rust HTTP/gRPC server. Composes framework crates + domain modules. Has a `main.rs`.
> This file is the **lean entry point**. Detailed architecture rules live under [`docs/architecture/`](docs/architecture/README.md).

**Read this first for any non-trivial work:**

- [`docs/architecture/README.md`](docs/architecture/README.md) — index of all architecture docs.
- [`docs/architecture/ai-guidelines.md`](docs/architecture/ai-guidelines.md) — MUST/SHOULD/NEVER rules every AI assistant follows here.
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — what this service is and how it fits a metaphor workspace.
- [`docs/architecture/layers.md`](docs/architecture/layers.md) — Clean Architecture layout, dependency rule, request flow.
- [`docs/architecture/ddd.md`](docs/architecture/ddd.md) — bounded contexts, entities, repositories, application services.
- [`docs/architecture/solid.md`](docs/architecture/solid.md) — SOLID with codebase-grounded examples.
- [`docs/architecture/clean-code.md`](docs/architecture/clean-code.md) — naming, file size (500-line split), errors, comments.

## What this is

An Axum + SQLx + Tokio binary that bootstraps the server: loads config, connects the DB, runs migrations through `MigrationManager`, composes `{Domain}Module` libraries into a single router, wires health checks + maintenance gate + audit logging, starts listening. Business logic lives in `module` projects — this service only composes.

When scaffolding a new project from this skeleton, the placeholder convention is:

- `{{service_name}}` — the binary/package name to use in `Cargo.toml` (default in this skeleton: `backbone-app`).
- `{{ServiceName}}` — the PascalCase form.
- `{{module_name}}` — a domain module composed into the service.

The skeleton ships with literal `backbone-app` in load-bearing Rust files so the repo stays runnable. Placeholders appear only in this document, in [`docs/architecture/`](docs/architecture/), in [`metaphor.codegen.yaml`](metaphor.codegen.yaml), and in commented example config.

## Golden path

```bash
# inside a metaphor consumer workspace
metaphor dev serve                        # REST + gRPC on default ports
metaphor dev serve --rest-only --port 8080
metaphor dev db migrate                   # apply module migrations
metaphor dev test --integration-only      # end-to-end tests
metaphor lint check                       # clippy + fmt + audit

# standalone (this skeleton, no workspace)
cargo run                                 # serve
cargo run -- migrate                      # placeholder migration entrypoint
cargo run -- healthcheck                  # probe /health (used by Docker HEALTHCHECK)
```

## Rules

- **MUST** have a `main.rs` and `[[bin]]` target.
- **MUST** load config from `config/application*.yml` via the framework config loader; no hardcoded secrets.
- **MUST** register every domain module in `main.rs` via its builder (`AccountingModule::builder().with_database(pool).build()?`) and merge its router.
- **MUST** apply migrations via [`backbone_orm::migrations::MigrationManager`](src/infrastructure/database/migrations/) (already wired in [`src/main.rs`](src/main.rs)) or `metaphor migration run-all` — never hand-ordered `sqlx::migrate!`. The skeleton's `cargo run -- migrate` subcommand is a placeholder; in real services it should delegate to `metaphor migration run-all` so module migrations are applied in the correct order.
- **MUST** use `BackboneCrudHandler` / `GenericCrudService` / `GenericCrudRepository` wiring; don't hand-roll CRUD routes.
- **NEVER** put business logic here. Feature work belongs in the owning `module` project.
- **NEVER** hand-write SQL migrations when the module's schema YAML can regenerate them. Edit `schema/models/*.model.yaml` upstream instead.
- **MUST** declare hand-written files inside generator-owned trees under [`metaphor.codegen.yaml`](metaphor.codegen.yaml) → `user_owned:`. See *Regen safety* below.
- **MUST** wrap any hand edit inside a generator-emitted file in `// <<< CUSTOM ... // END CUSTOM` markers. Edits outside markers are wiped on regen.
- **SHOULD** expose `/health`, `/readyz`, `/metrics` (Prometheus) and structured JSON logs. The skeleton wires `/health` plus a maintenance gate (`/maintenance/status`, `POST /maintenance`) out of the box.
- **SHOULD** feature-gate optional transports (`grpc`, `graphql`) when the module supports them.

## Folder cheatsheet

```
src/
├── main.rs                # bootstrap: config → pool → migrations → modules → router → listen
├── configuration/         # AppConfig loader (YAML + ${VAR} env overlay, dev-default warnings)
├── infrastructure/
│   └── database/          # DatabaseManager, pool prewarm, MigrationManager
├── middleware/            # CORS (others wired via tower in main.rs)
└── shared/                # AppState, error envelope, response shape, pagination

config/
├── application.yml        # base
├── application-dev.yml    # APP_ENV=dev overlay (ships with skeleton)
└── application-prod.yml   # APP_ENV=prod overlay (env-driven; ships with skeleton)

migrations/                # applied externally by `metaphor migration run-all` (and in-process by MigrationManager); generated from schema/models/*.model.yaml when modules are added

metaphor.codegen.yaml      # user_owned: globs the schema generator must never touch
docker-compose.yml         # local dependencies (postgres etc.)
Cargo.toml                 # deps: backbone-* crates + domain modules
```

The starter ships only the folders above. Domain layers (`src/domain/`, `src/application/`, `src/infrastructure/persistence/`, `src/presentation/`, `src/routes/`, `src/bootstrap/`, `src/exports/`, `src/handlers/`, `src/custom/`, `src/seeders/`, `src/subscriptions/`) materialise once you add a module or run `metaphor schema generate`. See [`docs/architecture/layers.md`](docs/architecture/layers.md) for the full layout.

## Tech stack (non-negotiable)

- HTTP: **Axum 0.7**
- gRPC: **Tonic 0.12** + Prost 0.13 (feature-gated)
- DB: **PostgreSQL** via **SQLx 0.8** (compile-time checked queries)
- Async: **Tokio 1.x** (full features)
- Serialization: serde / serde_json / serde_yaml
- Errors: `thiserror` for typed boundaries, `anyhow` inside `main.rs`
- Observability: `tracing` + `tracing-subscriber` (JSON in prod), Prometheus metrics via `backbone-observability`

## Local dev

Two compose paths ship with the skeleton — both are functional, pick the one that matches your workflow.

| Use this when… | Command | Compose file | Env file |
|---|---|---|---|
| Quickest one-liner; cargo runs on the host, only postgres/redis/minio in containers. | `docker compose up` (root) | [`docker-compose.yml`](docker-compose.yml) | inline (no env file needed) |
| You want the production-shape stack with hot-reload, project-named volumes, and `${VAR:?}` env validation. | `docker compose -f deployment/compose.dev.yaml --env-file deployment/.env.dev up -d` | [`deployment/compose.dev.yaml`](deployment/compose.dev.yaml) | [`deployment/.env.dev`](deployment/.env.dev) (copy from `.env.dev.example`) |
| Workspace-orchestrated dev (only when this app sits inside a metaphor consumer workspace under `apps/<service>/`). | `metaphor docker up --env dev` | workspace `deployment/compose.dev.yaml` | workspace `.env.dev` |

> ⚠️ Running multiple compose files simultaneously collides on `127.0.0.1:5432`. Pick one.

## Production stack

[`deployment/`](deployment/) ships a single-host production-grade compose stack: postgres + redis + minio + Caddy (TLS via Let's Encrypt) + Prometheus + Loki + Promtail + Grafana (provisioned datasources, dashboards, alerts) + cadvisor + node-exporter + postgres-exporter + uptime-kuma. See [`deployment/README.md`](deployment/README.md) for the operator runbook.

Two env files, two ownerships:
- [`deployment/.env.prod`](deployment/.env.prod.example) — orchestration (image tags, postgres init, MinIO root creds, Grafana, SMTP, edge `DOMAIN`).
- [`.env.prod`](.env.prod.example) at the repo root — service runtime (JWT secrets, MinIO access creds, CORS, rate limits, cache, feature flags).

Both files are validated by [`scripts/preflight-prod.sh`](scripts/preflight-prod.sh) before any sync to the VPS. Image releases are cut by [`.github/workflows/release-image.yml`](.github/workflows/release-image.yml) on `git tag v*` push.

## Common tasks

- "Add a new domain endpoint" → go to the owning `module` project, edit schema YAML there, regenerate, then here just register the module in `main.rs` if not already.
- "Bump a module version" → update dep in `Cargo.toml`, `metaphor dev db migrate`, `metaphor dev test`.
- "Run locally" → quickest: `docker compose up && cargo run`. Production-shape: `docker compose -f deployment/compose.dev.yaml --env-file deployment/.env.dev up -d`. Workspace: `metaphor docker up --env dev`.
- "Cut a release" → `git tag v0.5.2 && git push --tags`. CI builds the image and pushes to GHCR ([`.github/workflows/release-image.yml`](.github/workflows/release-image.yml)); deploy with `./scripts/deploy-service.sh backbone-app v0.5.2 SERVICE_TAG`.
- "Validate prod env before deploy" → `./scripts/preflight-prod.sh`.
- "Add rate limiting / auth middleware" → wire in the framework's middleware tower layer (`backbone-rate-limit`, `backbone-auth`); don't write it from scratch.

## Key files to read before editing

- [`src/main.rs`](src/main.rs) — bootstrap sequence; know what runs in what order.
- [`Cargo.toml`](Cargo.toml) — which framework crates and modules are composed (the skeleton ships with all 16 `backbone-*` crates and a `[patch]` block for local development).
- [`config/application.yml`](config/application.yml) — shape of config; env overlays in `application-dev.yml` / `application-prod.yml`.
- [`metaphor.codegen.yaml`](metaphor.codegen.yaml) — what the schema generator must NOT touch.
- Each imported module's `CLAUDE.md` — its rules apply to features it owns.

## Deeper knowledge (load on demand)

- Skill: `backbone-cli-master` — Backbone CLI surface + workflows.
- Skill: `backbone-modules-orchestrator` — composing modules into a service.
- Skill: `backbone-framework-architect` — framework crate layering.
- Skill: `api-and-interface-design` — REST/gRPC/GraphQL surface shape.
- Skill: `security-and-hardening` — authz, input validation, secret handling.

## Behavioural guidelines (summary)

Full rules: [`docs/architecture/ai-guidelines.md`](docs/architecture/ai-guidelines.md).

1. **Think before coding** — surface assumptions, present alternatives, ask when unclear.
2. **Simplicity first** — minimum code that solves the problem; no speculative abstractions.
3. **Surgical changes** — touch only what the request demands; match existing style; clean up only orphans your changes created.
4. **Goal-driven execution** — define verifiable success criteria; verify before claiming done.
5. **File granularity** — split any file > 500 lines along a real seam (responsibility / sub-module / entity); never `_part2.rs`-style splits.

## Regen safety

`metaphor schema generate [--force]` regenerates the bulk of `src/`, `migrations/`, `config/`, `tests/` from `schema/models/*.model.yaml`. There are **three preservation mechanisms** — know which protects what before adding hand-written code.

### 1. `metaphor.codegen.yaml` → `user_owned:`

Globs listed here are **skipped wholesale** on regen — never read, merged, or written. Used for files that exist inside the generator's output tree but aren't schema-derived. The shipped manifest is intentionally minimal:

- `src/main.rs`, `src/bootstrap/**`, `src/configuration/**`, `src/shared/**`, `src/middleware/**` — bootstrap / composition.
- `migrations/manual/**` — hand-written migrations.

**Add an entry whenever you create a new hand-written file inside a generator-owned tree.** Without it, the next `--force` will delete the file. Commented examples in [`metaphor.codegen.yaml`](metaphor.codegen.yaml) cover the common cases (custom services, custom infrastructure adapters, gRPC/GraphQL/CLI presentation, custom handlers, domain extensions, exports, subscriptions, seeders).

### 2. `// <<< CUSTOM ... // END CUSTOM` markers (inside generator-emitted files)

Generator-emitted aggregator files (e.g. `src/lib.rs`, `src/application/service/mod.rs`, `src/domain/repositories/mod.rs`) get rewritten on regen, but content between the markers is preserved. Use these for re-exports / mod declarations that need to live alongside generated content. Markers MUST contain complete syntactic units — full `pub mod foo;` / `pub use foo::*;` statements, not bare identifiers.

### 3. `-- Generated by metaphor-schema` marker (migrations only)

`metaphor schema generate --force` sweeps stale migrations in `migrations/`. Files containing the `-- Generated by metaphor-schema` header on line 1–10 are deletable; files without it survive automatically. So a brand-new hand-written migration in `migrations/` (without the header) is safe even without a `user_owned:` entry.

**Gotcha — stale-marker migrations:** a file that started life as generator output but is now hand-maintained still carries the marker and will be swept. Either remove the marker line, move the file to `migrations/manual/`, or pin its exact path under `user_owned:`.

### Regen checklist

After every `metaphor schema generate --force`:

1. `git status` — review every `D` (deleted) and `M` (modified). Anything you didn't expect to change → flag.
2. `cargo check` — verify compile **before** running anything else.
3. Spot-check the [`metaphor.codegen.yaml`](metaphor.codegen.yaml) paths in `git status` — they should NOT appear. If one does, the glob doesn't match.

## Anti-patterns

- Writing business logic in `main.rs` (belongs in a module).
- Hand-rolled `axum::Router` routes for CRUD (use `BackboneCrudHandler`).
- Hardcoded database URLs / secrets (use config + env).
- Skipping `MigrationManager` and running `sqlx migrate` ad-hoc.
- Blocking code inside Tokio tasks (`std::fs::read`, `reqwest::blocking`) — use async equivalents.
- Creating a new file inside `src/application/service/`, `src/domain/entity/`, `migrations/`, etc. without adding it to `user_owned:` in [`metaphor.codegen.yaml`](metaphor.codegen.yaml). The next `--force` regen will silently delete it.
- Hand-editing a `// Generated by metaphor-schema` file outside the `// <<< CUSTOM` markers. Those edits are clobbered on regen.
- Substituting `{{service_name}}` into [`Cargo.toml`](Cargo.toml) or [`src/main.rs`](src/main.rs) directly — those files use the literal name `backbone-app` so the skeleton stays runnable. Rename when scaffolding a new project, then drop the placeholders.
