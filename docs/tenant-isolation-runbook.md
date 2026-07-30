# Tenant-isolation runbook (RLS + verified principal)

The app's tenant boundary is **Postgres Row-Level Security** plus a **verified JWT principal**
(`company_auth`). This is how to turn it on in a real deployment, and what is already proven.

## How to enable (production)

1. **Apply module migrations as the OWNER/superuser** (they create the schemas/tables, the RLS
   `ENABLE`/`FORCE` policies, and the `asset.due_depreciation_assets` `SECURITY DEFINER` function).
   Run `metaphor migration run-all` (or `sqlx migrate run` per module) as `postgres`.
2. **Bootstrap the non-superuser app role** — idempotent; grants `USAGE` + DML on every non-system
   schema (including `asset` and `accounting`) and default privileges for future migrations:
   ```bash
   psql "$ADMIN_DATABASE_URL" \
       -v app_role=metaphor_app -v app_password='<strong>' \
       -v relay_role=metaphor_relay -v relay_password='<strong>' \
       -v owner_role=postgres \
       -f scripts/rls_app_role.sql
   ```
3. **Point the service at the app role** (keep migrations/seeders on the owner role):
   ```
   DATABASE_URL=postgresql://metaphor_app:<strong>@host:5432/backbone_app
   ```
4. `JWT_SECRET` is already read by the app to build the `CompanyVerifier` for `company_auth`.

> `metaphor_app` is created `NOSUPERUSER NOBYPASSRLS` — that is the whole point. A superuser
> connection (the dev default `postgres`) silently bypasses every RLS policy, so the fence looks
> installed but does nothing. The fence only binds under the non-super role.

## What's proven (2026-07-30)

- **Module contract** — backbone-asset's lifecycle handlers source `company_id` from the signed JWT
  (`CompanyContext`), never the request body; `load_asset(company_id, id)` scopes the read, so a
  cross-tenant `activate`/`depreciate`/`dispose` is `NotFound` → 404. (ADR-002; backbone-asset 0.4.x.)
- **RLS fence under the non-super role** — as `metaphor_app` (`NOSUPERUSER NOBYPASSRLS`) on
  `asset.assets`: own tenant → **1 row**; other tenant → **0 rows**; no `app.company_id` → **0 rows**
  (fail-closed). This is the exact cross-tenant read fence.
- **`company_auth` gate** — no token → **401**; bad token → **401**; valid JWT → handler reached.

## Startup DDL runs on an owner pool (`ADMIN_DATABASE_URL`)

The billing→tax dispatcher's `outbox::migrate` (and the module `MigrationManager`, and the per-schema
outbox migrates) do **owner-level DDL** — `CREATE SCHEMA`/`TABLE` and `ENABLE`/`FORCE ROW LEVEL
SECURITY` + `CREATE POLICY`, which require the table owner and cannot be granted to a non-owner role.
So the app runs all startup schema-DDL on a separate **admin/owner pool** sourced from
`ADMIN_DATABASE_URL`; the runtime `DATABASE_URL` (the `metaphor_app` role) is used only for fenced
runtime work. Dev (single-role, no `ADMIN_DATABASE_URL`) falls back to the runtime pool.

```bash
DATABASE_URL=postgresql://metaphor_app:<strong>@host/db          # runtime (NOSUPERUSER NOBYPASSRLS)
ADMIN_DATABASE_URL=postgresql://postgres:<strong>@host/db        # startup DDL only (owner)
```

## What's proven (2026-07-30, full app under `metaphor_app`)

The app boots as `metaphor_app` (with `ADMIN_DATABASE_URL` for startup DDL) and the fence holds
**live through the HTTP layer**:

- A's token → `POST /assets/register` → **201** (own-tenant create works under the non-super role).
- **B's token → `POST /assets/{A's id}/activate` → 404** (`load_asset` scopes to B → A's asset
  `NotFound`); A's asset stays `draft` — the cross-tenant write is refused, nothing changed.
- Plus the DB-level probe: own tenant → 1 row; other tenant → 0 rows; no scope → 0 rows.
