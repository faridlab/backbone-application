# Backbone

Minimal Backbone application skeleton.

A complete, runnable starter for the Backbone Framework that imports `backbone-framework` and boots an HTTP server with `/health`. Add business modules to it via `metaphor add module ... --to backbone` (when that command lands).

## Run

```bash
# 1. Start Postgres + Redis
docker-compose up -d

# 2. Run the app
cargo run

# 3. Hit it
curl http://localhost:8080/health
```

## What ships in the skeleton

- HTTP server (Axum) on port 8080 with `/health`
- Config loader (YAML + env): `config/application.yml` + `config/application-dev.yml`
- Postgres connection pool (`infrastructure::database::DatabaseManager`)
- Migration runner with framework base migrations (system_users, user_sessions, audit_logs, module_configurations)
- CORS middleware (configurable via env)
- Request logging middleware
- All 15 framework crates (`backbone-core`, `backbone-orm`, `backbone-auth`, etc.) imported and ready to use

## What's NOT in the skeleton

- No business modules (no sapiens, no bersihir, no bucket)
- No domain layer, no application layer, no presentation layer beyond `main.rs`
- No GraphQL, no gRPC
- No CLI subcommands
- No auth handlers (the auth crate is imported but no endpoints are mounted)

The skeleton is intentionally bare. Modules add their own routes, handlers, and migrations through the `register(&mut app)` pattern.

## Development

The `[patch]` block in `Cargo.toml` redirects framework dependencies to the local `../backbone-framework/` workspace. Delete that block once `backbone-framework` is published to GitHub.
