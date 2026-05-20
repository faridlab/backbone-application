# Overview — Backbone skeleton in the metaphor environment

## What this service is

A Rust HTTP/gRPC server that **composes** upstream framework crates and domain modules into a runnable binary. It owns no business logic of its own — feature work lives in upstream modules and is pulled in by version bump.

- HTTP: **Axum 0.7**
- gRPC: **Tonic 0.12** + Prost 0.13 (feature-gated)
- DB: **PostgreSQL** via **SQLx 0.8** (compile-time checked queries)
- Async: **Tokio 1.x** (full features)
- Observability: `tracing` + `tracing-subscriber` (JSON in prod), Prometheus via `backbone-observability`
- Errors: `thiserror` for typed boundaries, `anyhow` only inside `main.rs`

Entry point: [`src/main.rs`](../../src/main.rs). It runs: config → pool → migrations gate → module composition → router merge → listen.

## How this service sits in the metaphor workspace

When this skeleton is dropped into a metaphor **consumer workspace**, the shape is:

```
/path/to/{{workspace}}/                   ← consumer workspace
├── metaphor.yaml                         ← declares pinned upstream refs
├── metaphor.lock                         ← resolved commit SHAs (in git)
├── apps/                                 ← product code (what you edit)
│   └── {{service_name}}/                 ← THIS app (rendered from backbone)
└── modules/                              ← READ-ONLY upstream clones
    └── backbone-framework/               ← managed by `metaphor sync`
```

The standalone `backbone` repo itself is **not** a workspace — it's a single runnable app meant to be cloned, renamed, and dropped into a workspace under `apps/`.

### Hard rules (already in workspace CLAUDE.md — restated for AI)

- **NEVER** edit anything under `modules/*`. Changes are wiped by the next `metaphor sync`. To change upstream behaviour, fix it in the upstream repo, tag a release, bump the `ref:` in `metaphor.yaml`, then `metaphor sync --update`.
- **NEVER** hand-edit `metaphor.lock`. Regenerate via `metaphor sync --update`.
- **NEVER** copy files between `apps/*`. If they share code, promote it to a new or existing upstream module.
- **MUST** run `metaphor sync` after pulling changes that touch `metaphor.yaml` or `metaphor.lock`.

## Upstream crates this skeleton consumes

Pinned in [`Cargo.toml`](../../Cargo.toml) and (in a workspace) mirrored in `metaphor.yaml` — those two files are the source of truth for current versions. The list below names *which* crates we consume, not which tag is live.

| Group | Crates | What they give us |
|---|---|---|
| Core | `backbone-core` | `BackboneCrudHandler`, `GenericCrudService`, `GenericCrudRepository`, `ApiResponse`, common error envelope. |
| Persistence | `backbone-orm` | `DatabaseOperations<T>`, repository base, migration runner. |
| Web stack | `backbone-auth`, `backbone-authorization`, `backbone-rate-limit` | JWT auth, role/permission checks, login & API rate limiters. |
| Ops | `backbone-health`, `backbone-maintenance`, `backbone-observability` | `/health`, `/readyz`, `/metrics`, tracing, structured logs, audit middleware. |
| Messaging | `backbone-messaging`, `backbone-queue`, `backbone-jobs` | Event bus, queue abstractions, background jobs. |
| Storage | `backbone-storage`, `backbone-cache`, `backbone-search` | Object storage, cache, search index. |
| Comms | `backbone-email`, `backbone-graphql` | SMTP, GraphQL transport (feature-gated). |

Each is imported as a git crate; the [`[patch."https://github.com/faridlab/backbone-framework"]`](../../Cargo.toml) block in the skeleton's `Cargo.toml` redirects them to an in-tree workspace for local development — comment it out or pin to a release tag when building against published versions.

## Golden path (CLI)

Run from the workspace root unless noted.

```bash
metaphor doctor                          # tooling + upstream health
metaphor sync                            # clone/update remote modules to pinned refs
metaphor info                            # where am I in the workspace?
metaphor migration run-all               # apply all module migrations
metaphor docker up --env dev             # local dev stack (postgres + redis + minio + hot-reload service)
metaphor dev serve                       # run this service directly (no docker)
metaphor dev test --integration-only     # end-to-end tests
metaphor lint check                      # clippy + fmt + audit
```

For the local docker stack, **always pass `--env dev`** (or invoke `docker compose -f deployment/compose.dev.yaml --env-file deployment/.env.dev up -d` if you bypass metaphor). Running compose without the explicit env-file has leaked prod creds into the dev container before.

When working in the standalone skeleton (no workspace), the bare equivalents are:

```bash
cargo run                                # serve
cargo run -- migrate                     # placeholder migration entrypoint
cargo run -- healthcheck                 # probe /health
docker compose up                        # local dependencies (see docker-compose.yml)
```

## Per-stage entry points to read

When orienting in this service, read these in order:

1. [`src/main.rs`](../../src/main.rs) — bootstrap sequence; what runs in what order.
2. [`Cargo.toml`](../../Cargo.toml) — which modules and crates are actually composed.
3. [`config/application.yml`](../../config/application.yml) and the `application-${APP_ENV}.yml` overlay you'll run under.
4. The owning module's `CLAUDE.md` for the feature you're touching.
5. [`layers.md`](./layers.md) in this folder — to know which `src/` subfolder you should be editing.

## Where to put things (quick map)

| You want to… | Goes in… |
|---|---|
| Change business rules / entity invariants | the **upstream module** that owns the entity, not this service. |
| Add a new HTTP endpoint that wraps an existing service | `src/presentation/http/` (handler) + register in `src/routes/`. |
| Wire a new upstream module into the binary | `Cargo.toml` (dep) + `src/main.rs` (`Module::builder()...build()?` and merge its router). |
| Add a middleware | `src/middleware/` and apply via tower layer in `src/main.rs` / `src/routes/`. |
| Change config shape | `config/application.yml` (+ overlays) and the framework loader; no hardcoded reads. |
| Add a custom service that survives regeneration | `src/custom.rs` (or `src/custom/`) plus the `// <<< CUSTOM` guard pattern. |
| Add or change a SQL migration | the owning module's `schema/models/*.model.yaml` upstream — never hand-write SQL here. |

See [`layers.md`](./layers.md) for the full layer rules.
