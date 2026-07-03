# App-Developer Guide

> **Reader:** app developer — you're building a product on this skeleton, composing domain
> modules into a running service. **Mode:** tutorial (quickstart) then how-to (recipes).
> You do not need to read the architecture theory to finish the tasks on this page.

You get the smallest thing that runs first, then a set of "how do I…" recipes, then config and
troubleshooting. Every command here was run against this skeleton; where a command needs a
metaphor *workspace* that this standalone repo doesn't provide, it's marked **(workspace)**.

---

## 1. Install

**Prerequisites**

| Tool | Why | Check |
|---|---|---|
| Rust (stable, edition 2021) | builds the binary | `rustc --version` |
| Docker + Compose | Postgres/Redis/MinIO for local dev | `docker compose version` |
| `metaphor` CLI *(optional)* | workspace orchestration, codegen | `metaphor --version` → `metaphor 0.2.0` |

The skeleton composes the `backbone-*` crates from git at tag `v2.1.0` (see [`Cargo.toml`](../Cargo.toml)).
For local framework development a `[patch]` block redirects them to `../backbone-framework/`; comment
that block out to build strictly against the published tag.

```bash
git clone <this-repo> backbone-app && cd backbone-app
```

---

## 2. Quickstart — the smallest thing that runs

Two terminals. Postgres in Docker, the app on your host.

```bash
# 1. dependencies (postgres + redis + minio)
docker compose up -d

# 2. build & run the server (serve is the default subcommand)
cargo run

# 3. it's up — hit the health probe
curl http://localhost:8080/health
curl http://localhost:8080/maintenance/status
```

You should see the boot log in order — this **is** the bootstrap sequence in [`src/main.rs`](../src/main.rs):

```
🌀 Backbone skeleton starting
✅ Config loaded
✅ Database connected
✅ Migrations: <n> total, <m> pending
🚀 Listening on http://0.0.0.0:8080
```

That's the whole skeleton: config → pool → migrations → router → listen. It ships **no business
endpoints** — only `/health`, `/maintenance/status` (GET), and `/maintenance` (POST). You add
endpoints by composing modules (recipe 3).

### The three subcommands

| Command | Does | Notes |
|---|---|---|
| `cargo run` / `cargo run -- serve` | start the HTTP server | `serve` is the default |
| `cargo run -- healthcheck` | probe `/health`, exit 0 on 2xx | used by the Docker `HEALTHCHECK` |
| `cargo run -- migrate` | **placeholder** | prints a warning and exits 0; real services delegate to `metaphor migration run-all` |

> `migrate` is deliberately a no-op stub. In a real service, module migrations are applied in the
> correct cross-module order by `metaphor migration run-all` — never by a hand-ordered `sqlx::migrate!`.
> See [`CLAUDE.md` → Regen safety](../CLAUDE.md).

---

## 3. Key concept: this service *composes*, it doesn't *implement*

The one idea that makes everything else make sense:

> **Feature code lives in `module` projects upstream. This service pulls them in by version and
> merges their routers.** You almost never write domain logic here — you wire.

```
domain module (upstream)          this service (here)
─────────────────────────         ───────────────────────────
schema/models/*.model.yaml   ──▶   Cargo.toml  (add the dep)
generated entities/CRUD      ──▶   src/main.rs (Module::builder()...build()? + merge router)
generated migrations         ──▶   metaphor migration run-all
```

So the two recipes you'll use most are **"add a module"** and **"change config"** — not
"write a handler."

---

## 4. Recipes ("how do I…")

### Recipe: add a domain module to the service

1. **Add the dependency** in [`Cargo.toml`](../Cargo.toml), matching the tag the other crates use:
   ```toml
   backbone-accounting = { git = "https://github.com/faridlab/backbone-framework", tag = "v2.1.0" }
   ```
2. **Register it** in [`src/main.rs`](../src/main.rs) after the pool is ready, then merge its router:
   ```rust
   let accounting = AccountingModule::builder()
       .with_database(database.pool().clone())
       .build()?;
   let mut app = Router::new()
       .merge(health_routes(health_checker))
       .merge(maintenance_router)
       .merge(accounting.router());   // ← the module's routes
   ```
3. **Apply its migrations** — **(workspace)** `metaphor migration run-all`.
4. **Verify** — `cargo run`, then `curl` one of the module's endpoints and check the response shape.

> The builder pattern (`Module::builder().with_database(pool).build()?`) is mandatory — don't
> hand-mount a module's routes. See the module's own `CLAUDE.md` for its builder options.

### Recipe: run against the production-shape dev stack

The root `docker compose up` runs cargo on your host. When you want the production topology
(hot-reload container, named volumes, `${VAR:?}` validation):

```bash
cp deployment/.env.dev.example deployment/.env.dev   # first time only
docker compose -f deployment/compose.dev.yaml --env-file deployment/.env.dev up -d
```

> ⚠️ **Pick one compose path.** The root file and `deployment/compose.dev.yaml` both bind
> `127.0.0.1:5432` — running both at once collides on Postgres.

**(workspace)** If this app sits inside a metaphor consumer workspace under `apps/<service>/`,
prefer `metaphor docker up --env dev` — always pass `--env dev` so dev creds, not prod, load.

### Recipe: change a configuration value

Config is layered: base [`config/application.yml`](../config/application.yml), overlaid by
`config/application-${APP_ENV}.yml`, with every value overridable by env var via `${VAR:default}`.

```bash
# override the DB URL for one run without editing any file
DATABASE_URL=postgres://user:pw@db:5432/app cargo run

# switch overlays
APP_ENV=prod cargo run     # loads application-prod.yml on top of application.yml
```

The shape you can set (from [`config/application.yml`](../config/application.yml)):

| Section | Keys | Env override examples |
|---|---|---|
| `server` | `host`, `port` (8080) | `HOST`, — |
| `database` | `url`, `max_connections` (20), `min_connections` (5), timeouts | `DATABASE_URL` |
| `logging` | `level`, `format` (`json` in prod, `pretty` in dev) | `LOG_LEVEL`, `LOG_FORMAT` |
| `security` | `jwt_secret`, `jwt_algorithm`, `jwt_issuer`, `jwt_audience`, `jwt_expiration`, `cors_origins` | `JWT_SECRET`, … |

On boot the app calls `validate_defaults(&env)` and **warns** when a dev-default value (e.g.
`jwt_secret: change-me-in-production`) leaks into a non-dev environment. Heed those warnings before deploying.

### Recipe: add CORS / auth / rate-limit middleware

Don't write it from scratch — the framework ships tower layers. CORS is already wired via
[`src/middleware/cors.rs`](../src/middleware/cors.rs) (`default_cors_layer()`), applied in
`main.rs`. To add auth or rate limiting, layer the framework crates (`backbone-auth`,
`backbone-rate-limit`) in the same tower stack. Order matters — see the middleware ordering
comments in [`src/main.rs`](../src/main.rs) (maintenance gate outermost, audit innermost).

### Recipe: run the tests / lint before you push

```bash
metaphor lint check                    # clippy + fmt + audit   (workspace)
metaphor dev test                      # unit + integration     (workspace)
metaphor dev test --integration-only   # end-to-end             (workspace)
```

Standalone (no workspace): `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`.

### Recipe: build and cut a release image

```bash
git tag v0.5.2 && git push --tags       # CI builds + pushes the image to GHCR
./scripts/deploy-service.sh backbone-app v0.5.2 SERVICE_TAG   # deploy to the VPS
./scripts/preflight-prod.sh             # validate prod env files before any deploy
```

The image is cut by [`.github/workflows/release-image.yml`](../.github/workflows/release-image.yml)
on a `v*` tag push. The production compose stack lives in [`deployment/`](../deployment/README.md).

---

## 5. Configuration reference (quick)

- **Base:** [`config/application.yml`](../config/application.yml) — skeleton defaults.
- **Overlay:** `config/application-dev.yml` (auto when `APP_ENV=dev`, the default) /
  `config/application-prod.yml` (`APP_ENV=prod`, env-driven).
- **Override:** any `${VAR:default}` value via environment variable.
- **Runtime secrets** (JWT, MinIO creds, CORS, rate limits) belong in `.env.prod` at the repo root;
  orchestration secrets (image tags, Postgres init, Grafana) belong in `deployment/.env.prod`. Both
  are validated by [`scripts/preflight-prod.sh`](../scripts/preflight-prod.sh). Copy from the
  `*.example` files; never commit real secrets.

---

## 6. Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `failed to connect to database` on boot | Postgres not up, or `DATABASE_URL` points nowhere | `docker compose up -d`; confirm the URL/port; check `5432` isn't taken by a second compose stack |
| Boot logs warn about `change-me-in-production` | dev-default secret in a non-dev env | set a real `JWT_SECRET` (env or overlay) before deploying |
| Port 5432 already in use | both compose paths running | stop one — the root file and `deployment/compose.dev.yaml` collide on Postgres |
| `curl /health` refused | app not listening yet, or bound to a different host | wait for `🚀 Listening on …`; in dev the overlay binds `127.0.0.1`, base binds `0.0.0.0` |
| `unknown subcommand '<x>'` | typo — only `serve`, `migrate`, `healthcheck` exist | use one of the three (see §2) |
| `migrate` "did nothing" | it's a placeholder by design | use `metaphor migration run-all` **(workspace)** to apply module migrations |
| Module endpoint 404s after adding the dep | router not merged, or migrations not run | confirm the `.merge(module.router())` call in `main.rs`; run migrations |
| Compile errors after a schema regen | generator deleted a hand-written file | check `git status` for unexpected `D`; add the file's path to `metaphor.codegen.yaml` → `user_owned:` |

Deeper failures (build breaks, flaky migrations) → the systematic path is in
[`docs/architecture/ai-guidelines.md` §8](./architecture/ai-guidelines.md) and the
`debugging-and-error-recovery` skill.

---

## Where to go next

- Extending the service safely (regen, CUSTOM markers, where code goes) → [Maintainer guide](./maintainer-guide.md).
- The layer rules behind the recipes → [Architecture: layers](./architecture/layers.md).
- What a word means → [Glossary](./glossary.md).
