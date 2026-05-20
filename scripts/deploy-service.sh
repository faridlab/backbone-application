#!/usr/bin/env bash
# Deploy a single service to prod by pulling a pre-built image from GHCR.
#
# Use when the image is already in GHCR (e.g. CI just built and pushed
# backbone-app:v0.5.2). Skips the local rebuild that
# `metaphor deploy push` would do, which would otherwise overwrite the
# CI-built image with whatever your laptop produces.
#
# Usage:
#   ./scripts/deploy-service.sh backbone-app v0.5.2 SERVICE_TAG
#
# Env overrides:
#   HOST=deploy@example.com
#   COMPOSE_DIR=/srv/backbone/deployment    # where compose.yaml lives on VPS
#   ENV_FILE_REMOTE=.env.prod               # relative to COMPOSE_DIR

set -euo pipefail

if [ "$#" -ne 3 ]; then
  cat >&2 <<USAGE
Usage: $0 <service-name> <tag> <env-var-name>

Example:
  $0 backbone-app v0.5.2 SERVICE_TAG

Service name must match a key under \`services:\` in deployment/compose.yaml.
Env var name must match the variable referenced by that service's image: line.
USAGE
  exit 1
fi

SERVICE="$1"
TAG="$2"
ENV_VAR="$3"

HOST="${HOST:-deploy@example.com}"
COMPOSE_DIR="${COMPOSE_DIR:-/srv/backbone/deployment}"
ENV_FILE_REMOTE="${ENV_FILE_REMOTE:-.env.prod}"

echo "→ Updating $ENV_VAR=$TAG on $HOST and rolling $SERVICE"

ssh "$HOST" bash -se <<REMOTE
set -euo pipefail
cd "$COMPOSE_DIR"

# Capture the previous tag for the recap line at the end.
prev=\$(grep -E "^${ENV_VAR}=" "$ENV_FILE_REMOTE" | head -1 | cut -d= -f2-)

# Atomic edit: write to a temp file alongside, then mv. Avoids a partial
# write if sed is interrupted.
tmp=\$(mktemp "$ENV_FILE_REMOTE.XXXXXX")
sed "s|^${ENV_VAR}=.*|${ENV_VAR}=${TAG}|" "$ENV_FILE_REMOTE" > "\$tmp"
chmod --reference="$ENV_FILE_REMOTE" "\$tmp"
mv "\$tmp" "$ENV_FILE_REMOTE"

echo "  $ENV_VAR: \${prev:-<unset>} → ${TAG}"

# Pull just the one image, then `up -d` only that service. Compose only
# recreates if the digest differs, so this is idempotent.
docker compose --env-file "$ENV_FILE_REMOTE" pull "$SERVICE"
docker compose --env-file "$ENV_FILE_REMOTE" up -d "$SERVICE"

# Wait briefly for the container to settle, then show health.
sleep 2
docker compose --env-file "$ENV_FILE_REMOTE" ps "$SERVICE"
REMOTE

echo ""
echo "✓ $SERVICE deployed at $TAG"
echo "  Verify: curl https://api.\$DOMAIN/health    # expects { version: \"${TAG#v}\", ... }"
