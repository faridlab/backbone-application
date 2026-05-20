# Backbone — Operator Runbook

Single-host, single-service production stack for backbone-app. Local dev uses [compose.dev.yaml](compose.dev.yaml); this runbook covers the prod/uat VPS stack ([compose.yaml](compose.yaml)).

## Layout on the VPS

The VPS mirrors the project root so compose `env_file:` paths resolve identically locally and remote.

```
/srv/backbone/                       # project root on VPS
├── deployment/                      # mirrors repo deployment/
│   ├── compose.yaml
│   ├── .env.prod                    # orchestration config (chmod 600)
│   ├── caddy/Caddyfile
│   ├── prometheus/prometheus.yml
│   ├── loki/loki-config.yaml
│   ├── promtail/promtail-config.yaml
│   ├── grafana/provisioning/
│   └── backups/
│       ├── pg-backup.sh
│       └── restic-push.sh
└── .env.prod                        # service-owned runtime (chmod 600)
```

> **Two-file env model, single ownership.** `deployment/.env.prod` holds
> orchestration (image tags, infra creds, edge config). The repo-root `.env.prod`
> holds runtime config the service binary needs (JWT secret, MinIO access creds,
> log level), with the contract committed at the repo-root `.env.prod.example`.
> Compose loads both via stacked `env_file:` entries. Run compose from
> `/srv/backbone/deployment/` on the VPS (not `/srv/backbone/`).

## First deploy

```bash
# On the laptop — build + push the service image.
SHA=$(git rev-parse --short HEAD)
DOMAIN=example.com                              # your real apex domain

docker buildx build --platform linux/amd64 \
  -t ghcr.io/your-github-org/backbone-app:$SHA \
  -t ghcr.io/your-github-org/backbone-app:beta \
  --push .

# On the laptop — prepare both env files and validate locally first.
cp deployment/.env.prod.example deployment/.env.prod
chmod 600 deployment/.env.prod
$EDITOR deployment/.env.prod                              # fill DOMAIN / POSTGRES_* / MINIO_ROOT_* / SMTP_* / GHCR_OWNER / etc.
sed -i "s/^SERVICE_TAG=.*/SERVICE_TAG=$SHA/" deployment/.env.prod

cp .env.prod.example .env.prod
chmod 600 .env.prod
$EDITOR .env.prod                                          # fill JWT_SECRET / MINIO_ACCESS_KEY / MINIO_SECRET_KEY / CORS_ORIGIN

./scripts/preflight-prod.sh                                # validates BOTH files

# On the VPS — first deploy.
ssh deploy@vps "mkdir -p /srv/backbone/deployment"

# Sync deployment/ tree (rsync — only changed bytes; --dry-run for preview).
rsync -avz --delete \
  --exclude='.env.prod.example' --exclude='.gitignore' \
  deployment/ deploy@vps:/srv/backbone/deployment/

rsync -avz .env.prod deploy@vps:/srv/backbone/.env.prod

ssh deploy@vps
cd /srv/backbone/deployment

# Login to GHCR with a read-only PAT.
echo $GHCR_PAT_READONLY | docker login ghcr.io -u your-github-user --password-stdin

# Up the stack.
docker compose --env-file .env.prod pull
docker compose --env-file .env.prod up -d

# Sanity.
docker compose logs -f backbone-app
curl -fsS https://api.${DOMAIN}/health
```

## Update beta (roll forward)

Three deploy patterns depending on what changed.

### Single service from a CI-built image (fastest, doesn't rebuild)

```bash
# CI just pushed backbone-app:v0.5.2 to GHCR. Roll prod to it:
./scripts/deploy-service.sh backbone-app v0.5.2 SERVICE_TAG
# verify
curl https://api.example.com/health | jq '{ version, commit, built_at }'
```

### Bump tag locally, then push

```bash
./scripts/bump-prod-tag.sh SERVICE_TAG v0.5.2
./scripts/preflight-prod.sh                                # validate locally first
git diff deployment/.env.prod                              # review
git commit -am "chore(deploy): bump backbone-app v0.5.2"
# push via your usual deploy pipeline, or scp + docker compose pull/up on the VPS
```

### Manual (when scripts don't fit)

```bash
ssh deploy@vps
cd /srv/backbone/deployment
sed -i "s/^SERVICE_TAG=.*/SERVICE_TAG=v0.5.2/" .env.prod
docker compose --env-file .env.prod pull                   # only changed images get pulled
docker compose --env-file .env.prod up -d                  # only changed services restart
docker compose logs -f --tail=100 backbone-app
```

## Roll back

```bash
# Flip *_TAG entries in .env.prod to the previous SHA, then:
docker compose --env-file .env.prod pull
docker compose --env-file .env.prod up -d
```
Previous images stay in GHCR; don't prune aggressively.

## Migrations

The skeleton's `backbone-app migrate` subcommand is a **placeholder** — it logs a warning and exits 0 so compose's one-shot migrations container doesn't crash. The real migration runner is `backbone_orm::migrations::MigrationManager` (wired in `src/main.rs`), which applies migrations as part of the service startup. Until that path is also exposed via the `migrate` subcommand, run schema changes via:

```bash
# Tunnel through SSH (Postgres is bound to 127.0.0.1:5432 on the VPS).
ssh -N -L 5433:127.0.0.1:5432 deploy@vps &
DATABASE_URL=postgresql://$POSTGRES_USER:$POSTGRES_PASSWORD@127.0.0.1:5433/$POSTGRES_DB \
  metaphor migration run-all
```

## Monitoring

- Grafana: `https://grafana.${DOMAIN}` — initial admin login uses `GRAFANA_ADMIN_PASSWORD` from `.env.prod`.
- Three dashboards ship under the **Backbone** folder: Golden Signals, Postgres Health, Host & Container Resources.
- Five alerts ship under **Alerting → Alert rules → Backbone → golden-signals**: API 5xx rate, API p95 latency, Postgres connection saturation, host disk > 80 %, container restart loop. Rules live in `grafana/provisioning/alerting/rules.yml`.
- Alert routing: all alerts → `email-ops` contact point; `severity=critical` alerts repeat every 1h, `severity=warning` every 4h. The recipient address is hardcoded at `grafana/provisioning/alerting/contact-points.yml` (Grafana doesn't expand env vars inside provisioned alert settings) — edit the file and `docker compose restart grafana` to change it, or add extras through the Grafana UI.
- Community dashboards worth importing through the UI after first boot:
  - **Node Exporter Full** — Grafana.com ID `1860`
  - **Docker cAdvisor** — Grafana.com ID `14282`

> **Metric names unverified.** Alert rules and dashboards assume `http_requests_total` / `http_request_duration_seconds_bucket` from `backbone-observability`. If the actual emitted names differ (e.g. `axum_http_requests_total`), both the dashboards and rules need to be renamed in lockstep. Verify via `curl http://backbone-app:9090/metrics | grep -E '^# TYPE http'` once the service starts.

## Backups

Nightly `pg_dump`:
```bash
sudo install -m 0755 /srv/backbone/deployment/backups/pg-backup.sh /usr/local/bin/
sudo mkdir -p /var/backups/pg && sudo chown deploy:deploy /var/backups/pg
echo '15 2 * * * deploy /usr/local/bin/pg-backup.sh >> /var/log/pg-backup.log 2>&1' \
  | sudo tee /etc/cron.d/pg-backup
```

Weekly off-site push via restic:
```bash
# One-time: create /etc/default/restic (chmod 600) with RESTIC_REPOSITORY,
# RESTIC_PASSWORD, B2_ACCOUNT_ID, B2_ACCOUNT_KEY. Then:
sudo restic -r "$RESTIC_REPOSITORY" init
sudo install -m 0755 /srv/backbone/deployment/backups/restic-push.sh /usr/local/bin/
echo '30 3 * * 0 deploy /usr/local/bin/restic-push.sh >> /var/log/restic-push.log 2>&1' \
  | sudo tee /etc/cron.d/restic-push
```

Monthly repo integrity check (catches B2-side bit-rot before you need a restore):
```bash
sudo install -m 0755 /srv/backbone/deployment/backups/restic-check.sh /usr/local/bin/
{ echo 'MAILTO=ops@example.com';
  echo '0 4 1 * * deploy /usr/local/bin/restic-check.sh >> /var/log/restic-check.log 2>&1'; } \
  | sudo tee /etc/cron.d/restic-check
```

**Restore drill (run once before going live):**
```bash
latest=$(ls -1t /var/backups/pg/backbone-*.sql.gz | head -1)
docker compose exec -T postgres \
  psql -U $POSTGRES_USER -c "CREATE DATABASE backbone_restore;"
gunzip -c "$latest" \
  | docker compose exec -T postgres \
      psql -U $POSTGRES_USER -d backbone_restore
# Sanity-check a row count, then drop backbone_restore.
```

## Troubleshooting

- **`curl https://api.${DOMAIN}` returns TLS error** — DNS probably hasn't propagated; check `dig api.${DOMAIN}`. Caddy logs: `docker compose logs caddy`.
- **`backbone-app` restart loop** — `docker compose logs backbone-app`. Most common cause: `DATABASE_URL` wrong / postgres not yet ready; compose should handle ordering via `depends_on: condition: service_healthy`.
- **`bucket.${DOMAIN}/<key>` returns 302 to 127.0.0.1 or minio:9000** — `MINIO_PUBLIC_ENDPOINT` in `.env.prod` is wrong; should be `https://s3.${DOMAIN}`.
