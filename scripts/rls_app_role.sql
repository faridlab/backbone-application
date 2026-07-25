-- RLS application role bootstrap (ADR-0008)
-- ============================================
--
-- The company read-fence is Postgres Row-Level Security. RLS is enforced ONLY for a role that is
-- neither a superuser nor BYPASSRLS — a superuser connection (the common dev default,
-- `postgresql://postgres:...`) silently bypasses every policy, so the fence looks installed but does
-- nothing. This script creates the non-privileged role the *application* connects as; migrations and
-- seeders keep running as the owner/superuser and are the deliberately-unfenced system path.
--
-- Run ONCE per database, as the owner/superuser, AFTER the module migrations have created the
-- schemas and tables:
--
--     psql "$ADMIN_DATABASE_URL" \
--         -v app_role=metaphor_app -v app_password='change-me' \
--         -v relay_role=metaphor_relay -v relay_password='change-me' \
--         -v owner_role=postgres \
--         -f scripts/rls_app_role.sql
--
-- Then point the service at the app role instead of the superuser:
--
--     DATABASE_URL=postgresql://metaphor_app:change-me@host:5432/backbone_app
--
-- And point the outbox RELAY at its own role (it drains every tenant's outbox_events — see the
-- `OR current_user = 'metaphor_relay'` bypass in backbone_outbox's fence policy):
--
--     RELAY_DATABASE_URL=postgresql://metaphor_relay:change-me@host:5432/backbone_app
--
-- Idempotent: safe to re-run after adding modules/tables (re-grants current tables and refreshes
-- default privileges for future ones).
--
-- Note the `SELECT format(...) \gexec` idiom throughout: psql does NOT substitute `:vars` inside a
-- `DO $$ … $$` block (it is a dollar-quoted string), so parameterised DDL is built as a string at the
-- top level and executed with \gexec — one generated statement per returned row.

\set ON_ERROR_STOP on

-- 1. The login role: NOSUPERUSER + NOBYPASSRLS are the whole point — do not grant either.
--    Create only if absent...
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE',
              :'app_role', :'app_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'app_role')
\gexec

--    ...then always refresh the password and hard-guarantee the no-bypass attributes.
SELECT format('ALTER ROLE %I LOGIN PASSWORD %L NOSUPERUSER NOBYPASSRLS', :'app_role', :'app_password')
\gexec

SELECT format('GRANT CONNECT ON DATABASE %I TO %I', current_database(), :'app_role')
\gexec

-- 1b. The outbox RELAY login role. The relay drains outbox_events across ALL tenants (it is cross-tenant
--     by nature). Like the app role it is NOSUPERUSER + NOBYPASSRLS — it gains cross-tenant visibility
--     ONLY via the `OR current_user = 'metaphor_relay'` clause in backbone_outbox's fence policy, NOT via
--     a bypass attribute, so every OTHER table's fence still applies to it. Least privilege: it touches
--     outbox_events only. Re-run this script after adding a module so its outbox_events is granted.
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE',
              :'relay_role', :'relay_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'relay_role')
\gexec

SELECT format('ALTER ROLE %I LOGIN PASSWORD %L NOSUPERUSER NOBYPASSRLS', :'relay_role', :'relay_password')
\gexec

SELECT format('GRANT CONNECT ON DATABASE %I TO %I', current_database(), :'relay_role')
\gexec

-- USAGE on each producer schema + SELECT/UPDATE on outbox_events only (read a batch, mark published_at).
SELECT format('GRANT USAGE ON SCHEMA %I TO %I', nspname, :'relay_role')
FROM pg_namespace WHERE nspname NOT LIKE 'pg\_%' AND nspname <> 'information_schema'
\gexec

SELECT format('GRANT SELECT, UPDATE ON %I.outbox_events TO %I', nspname, :'relay_role')
FROM pg_namespace n
WHERE n.nspname NOT LIKE 'pg\_%' AND n.nspname <> 'information_schema'
  AND EXISTS (SELECT 1 FROM information_schema.tables t
              WHERE t.table_schema = n.nspname AND t.table_name = 'outbox_events')
\gexec

-- 2. USAGE + DML on every non-system schema. Reads/writes are still fenced by RLS per row; these
--    grants only decide which *tables* the role may touch at all.
SELECT format('GRANT USAGE ON SCHEMA %I TO %I', nspname, :'app_role')
FROM pg_namespace WHERE nspname NOT LIKE 'pg\_%' AND nspname <> 'information_schema'
\gexec

SELECT format('GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA %I TO %I', nspname, :'app_role')
FROM pg_namespace WHERE nspname NOT LIKE 'pg\_%' AND nspname <> 'information_schema'
\gexec

SELECT format('GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA %I TO %I', nspname, :'app_role')
FROM pg_namespace WHERE nspname NOT LIKE 'pg\_%' AND nspname <> 'information_schema'
\gexec

-- 3. DEFAULT PRIVILEGES so tables/sequences created by FUTURE migrations (run by the owner) are
--    automatically reachable by the app role — no re-run needed after each new migration.
SELECT format('ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA %I GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO %I',
              :'owner_role', nspname, :'app_role')
FROM pg_namespace WHERE nspname NOT LIKE 'pg\_%' AND nspname <> 'information_schema'
\gexec

SELECT format('ALTER DEFAULT PRIVILEGES FOR ROLE %I IN SCHEMA %I GRANT USAGE, SELECT ON SEQUENCES TO %I',
              :'owner_role', nspname, :'app_role')
FROM pg_namespace WHERE nspname NOT LIKE 'pg\_%' AND nspname <> 'information_schema'
\gexec

\echo 'RLS app role bootstrapped. Point DATABASE_URL at it; keep migrations/seeders on the owner role.'
