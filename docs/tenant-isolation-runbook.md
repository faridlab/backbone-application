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

## Known follow-up — blocks full-app boot as `metaphor_app`

The billing→tax dispatcher runs `backbone_outbox::outbox::migrate(pool, "billing")` on the app's
**runtime pool** at startup. That migrate does owner-level DDL (`CREATE SCHEMA`, `CREATE TABLE`, and
owner-only index/trigger work — `must be owner of table outbox_events`) that a non-owner role cannot
perform. So the full app **cannot boot as `metaphor_app`** until that migrate is moved to an
owner/admin connection (or guarded to a no-op when the schema already exists). This is a
skeleton-wide concern, not asset-specific. Until it lands, the verified-principal + RLS fence is
proven at the module + DB level (above), while the dev app still boots as the superuser.
