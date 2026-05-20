#!/usr/bin/env bash
# Bump a *_TAG value in deployment/.env.prod (locally — does NOT deploy).
#
# Pair with deploy-service.sh once you've bumped one or more tags.
#
# Usage:
#   ./scripts/bump-prod-tag.sh SERVICE_TAG v0.5.2
#
# Doesn't auto-commit — leaves the change unstaged so you can review the
# diff and bundle multiple bumps into one commit before pushing.

set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "Usage: $0 <ENV_VAR_NAME> <new-tag>" >&2
  echo "Example: $0 SERVICE_TAG v0.5.2" >&2
  exit 1
fi

ENV_VAR="$1"
NEW_TAG="$2"
ENV_FILE="${ENV_FILE:-deployment/.env.prod}"

cd "$(dirname "$0")/.."

if [ ! -f "$ENV_FILE" ]; then
  echo "ERROR: $ENV_FILE not found." >&2
  exit 1
fi

if ! grep -qE "^${ENV_VAR}=" "$ENV_FILE"; then
  echo "ERROR: ${ENV_VAR} not found in $ENV_FILE." >&2
  echo "       Add it manually first if this is a new var." >&2
  exit 1
fi

prev=$(grep -E "^${ENV_VAR}=" "$ENV_FILE" | head -1 | cut -d= -f2-)

if [ "$prev" = "$NEW_TAG" ]; then
  echo "→ $ENV_VAR already at $NEW_TAG. No change."
  exit 0
fi

# Portable in-place sed (works on both macOS and GNU sed).
tmp=$(mktemp "${ENV_FILE}.XXXXXX")
sed "s|^${ENV_VAR}=.*|${ENV_VAR}=${NEW_TAG}|" "$ENV_FILE" > "$tmp"
mv "$tmp" "$ENV_FILE"

echo "✓ $ENV_VAR: ${prev:-<unset>} → $NEW_TAG"
echo ""
echo "Next steps:"
echo "  1. Review:           git diff $ENV_FILE"
echo "  2. Validate locally: ./scripts/preflight-prod.sh"
echo "  3. Deploy:           ./scripts/deploy-service.sh <service> $NEW_TAG $ENV_VAR"
