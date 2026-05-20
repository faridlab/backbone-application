#!/usr/bin/env bash
# Validate prod env files before scp/ssh to the VPS.
#
# Two layers of checking:
#   1. Compose-level: `docker compose config` resolves all ${VAR:?…} refs
#      in deployment/compose.yaml against deployment/.env.prod. Catches
#      missing image tags + infra-orchestration vars.
#   2. Service contract: every var listed in the service's .env.prod.example
#      (the contract owned by the service repo) MUST be present and
#      non-placeholder in the repo-root .env.prod.
#
# Usage:
#   ./scripts/preflight-prod.sh                       # validate .env.prod + service files
#   ENV_FILE=deployment/.env.uat ./scripts/preflight-prod.sh

set -euo pipefail

cd "$(dirname "$0")/.."

ENV_FILE="${ENV_FILE:-deployment/.env.prod}"
COMPOSE_FILE="${COMPOSE_FILE:-deployment/compose.yaml}"

# Map of service-runtime files to their contract files.
# Format: <service-name>:<contract-file>:<actual-file>
declare -a SERVICES=(
  "backbone-app:.env.prod.example:.env.prod"
)

# Placeholder values that should never reach prod. Matches prefix
# (CHANGE_ME_TO_A_LONG_RANDOM_STRING) and angle-bracket (<your-key-here>) styles.
PLACEHOLDER_PATTERN='^(CHANGE_ME|TODO|REPLACE_ME|FILL_|XXX+)|<[^>]+>'

# ─── Layer 0 — File existence + readability ────────────────────────────
echo "→ [1/3] File existence check"
overall_failed=0

if [ ! -f "$ENV_FILE" ]; then
  echo "  ✗ $ENV_FILE not found" >&2
  echo "    → cp deployment/.env.prod.example $ENV_FILE && chmod 600 $ENV_FILE && \$EDITOR $ENV_FILE" >&2
  exit 1
fi
echo "  ✓ $ENV_FILE"

for entry in "${SERVICES[@]}"; do
  service_name="${entry%%:*}"
  rest="${entry#*:}"
  contract_file="${rest%%:*}"
  actual_file="${rest##*:}"

  if [ ! -f "$contract_file" ]; then
    echo "  ✗ Contract missing for $service_name: $contract_file" >&2
    overall_failed=1
    continue
  fi
  if [ ! -f "$actual_file" ]; then
    echo "  ✗ Actual env file missing for $service_name: $actual_file" >&2
    echo "    → cp $contract_file $actual_file && chmod 600 $actual_file && \$EDITOR $actual_file" >&2
    overall_failed=1
    continue
  fi
  echo "  ✓ $actual_file (contract: $contract_file)"
done

[ "$overall_failed" -eq 0 ] || exit 1

# ─── Layer 1 — Per-service contract check ──────────────────────────────
echo "→ [2/3] Service contract check"
for entry in "${SERVICES[@]}"; do
  service_name="${entry%%:*}"
  rest="${entry#*:}"
  contract_file="${rest%%:*}"
  actual_file="${rest##*:}"

  required_vars=$(grep -E '^[A-Z_]+=' "$contract_file" | cut -d= -f1)
  missing=()
  placeholders=()
  while IFS= read -r var; do
    val=$(grep -E "^${var}=" "$actual_file" | head -1 | cut -d= -f2-)
    if [ -z "$val" ]; then
      missing+=("$var")
    elif [[ "$val" =~ $PLACEHOLDER_PATTERN ]]; then
      placeholders+=("$var=$val")
    fi
  done <<< "$required_vars"

  if [ ${#missing[@]} -gt 0 ] || [ ${#placeholders[@]} -gt 0 ]; then
    echo "  ✗ $service_name:" >&2
    if [ ${#missing[@]} -gt 0 ]; then
      for v in "${missing[@]}";      do echo "      [missing]     $v" >&2; done
    fi
    if [ ${#placeholders[@]} -gt 0 ]; then
      for v in "${placeholders[@]}"; do echo "      [placeholder] $v" >&2; done
    fi
    overall_failed=1
  else
    count=$(echo "$required_vars" | wc -l | tr -d ' ')
    echo "  ✓ $service_name: all $count contract vars present and non-placeholder"
  fi
done

[ "$overall_failed" -eq 0 ] || exit 1

# ─── Layer 2 — Compose-level interpolation check ───────────────────────
echo "→ [3/3] Compose-level check (resolves all \${VAR:?...} interpolations)"
if ! docker compose -f "$COMPOSE_FILE" --env-file "$ENV_FILE" config > /dev/null 2>&1; then
  echo ""
  echo "✗ Compose validation failed:" >&2
  docker compose -f "$COMPOSE_FILE" --env-file "$ENV_FILE" config 2>&1 >/dev/null \
    | grep -E "^error|must be set|missing" \
    | head -10
  exit 1
fi
echo "  ✓ all compose-interpolated vars resolve in $ENV_FILE"

echo ""
echo "✓ Preflight passed. Safe to sync to the VPS."
echo ""
echo "  # First, ensure the directory layout exists:"
echo "  ssh deploy@vps 'mkdir -p /srv/backbone/deployment'"
echo ""
echo "  # Sync deployment tree (--delete cleans up files removed locally;"
echo "  # --dry-run first to preview):"
echo "  rsync -avz --delete --exclude='.env.prod.example' --exclude='.gitignore' \\"
echo "    deployment/ deploy@vps:/srv/backbone/deployment/"
echo ""
echo "  # Sync the service env file:"
for entry in "${SERVICES[@]}"; do
  actual_file="${entry##*:}"
  echo "  rsync -avz $actual_file deploy@vps:/srv/backbone/$actual_file"
done
