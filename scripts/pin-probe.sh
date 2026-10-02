#!/usr/bin/env bash
# Pin probe for the service's dependency graph.
#
# Every backbone crate reaches this service from crates.io by semver
# requirement (ADR-0030 in the metaphora handbook); Cargo.lock is the one
# place exact versions are pinned, so commit it. What it asserts:
#
#   1. No [patch] table in Cargo.toml, and no committed .cargo/config.toml
#      that patches. A local build may link a module checkout through the
#      untracked .cargo/config.toml `metaphor dev link` writes; a committed
#      one would silently redirect every build.
#   2. Every backbone-* dependency Cargo.toml declares is a crates.io
#      requirement: it carries a version and no git or path source.
#   3. cargo metadata --locked resolves the committed lockfile unchanged.
#   4. Every backbone-* package in Cargo.lock comes from the crates.io
#      index. A path-sourced copy (a `metaphor dev link`) is admitted only
#      outside --release, and only inside the metaphora checkout.
#   5. Every backbone-* name appears in the lock exactly once.
#   6. Every declared backbone dependency is present in the lock.
#   7. Single framework version: all framework crates resolve to one version.
#   8. Online only: no locked backbone version is yanked on crates.io.
#
# Product-specific graph rules (which family may depend on which) belong in
# the service's own copy; add them below the generic checks.
#
# Modes: --offline skips the crates.io check and runs cargo offline;
# --release refuses the local path-override posture.
#
# Usage:
#   scripts/pin-probe.sh              # full probe, online (default)
#   scripts/pin-probe.sh --offline    # no network
#   scripts/pin-probe.sh --release    # CI and release evidence
set -euo pipefail

MODE="online"
RELEASE="false"
for arg in "$@"; do
  case "$arg" in
    --offline) MODE="offline" ;;
    --release) RELEASE="true" ;;
    *) echo "unknown argument: $arg (supported: --offline, --release)" >&2; exit 2 ;;
  esac
done

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$REPO_DIR/Cargo.toml"
CARGO_LOCK="$REPO_DIR/Cargo.lock"
CRATES_IO='registry+https://github.com/rust-lang/crates.io-index'
FRAMEWORK_CRATES="backbone-auth backbone-authorization backbone-cache backbone-core backbone-email backbone-gl-posting backbone-graphql backbone-health backbone-jobs backbone-maintenance backbone-messaging backbone-observability backbone-orm backbone-outbox backbone-queue backbone-rate-limit backbone-search backbone-storage backbone-tenant"
METAPHORA_DIR="${PIN_PROBE_METAPHORA_DIR:-$(cd "$REPO_DIR/../../../.." >/dev/null 2>&1 && cd frameworks/metaphora >/dev/null 2>&1 && pwd || true)}"

fail() { echo "PIN PROBE FAIL: $*" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || fail "jq is required to read cargo metadata output (not on PATH)"
[ -f "$CARGO_LOCK" ] || fail "no Cargo.lock: an application commits its lockfile (run cargo generate-lockfile, then commit it)"

# 1. No committed [patch] table.
if grep -Eq '^[[:space:]]*\[patch' "$CARGO_TOML"; then
  fail "a [patch] table exists in Cargo.toml — override locally through an untracked .cargo/config.toml instead"
fi
echo "ok: no [patch] table in Cargo.toml"
if git -C "$REPO_DIR" ls-files --error-unmatch .cargo/config.toml >/dev/null 2>&1 \
   && grep -Eq '^[[:space:]]*\[patch' "$REPO_DIR/.cargo/config.toml"; then
  fail "a committed .cargo/config.toml carries a [patch] table — local links belong in the untracked file \`metaphor dev link\` writes"
fi
echo "ok: no committed cargo config patches a dependency"

# 2. Declared backbone dependencies are crates.io requirements.
declared=$(python3 - "$CARGO_TOML" <<'PY'
import sys, tomllib
t = tomllib.load(open(sys.argv[1], "rb"))
bad, names = [], []
for section in ("dependencies", "dev-dependencies", "build-dependencies"):
    for key, spec in t.get(section, {}).items():
        name = spec.get("package", key) if isinstance(spec, dict) else key
        if not name.startswith("backbone-"): continue
        names.append(name)
        if isinstance(spec, str): continue
        if "git" in spec or "path" in spec or "version" not in spec:
            bad.append(f"{section}.{key}")
if bad: print("BAD " + " ".join(bad))
print("NAMES " + " ".join(sorted(set(names))))
PY
)
bad_decl=$(printf '%s\n' "$declared" | sed -n 's/^BAD //p')
[ -z "$bad_decl" ] || fail "backbone dependencies not declared as crates.io requirements: $bad_decl"
DECLARED_NAMES=$(printf '%s\n' "$declared" | sed -n 's/^NAMES //p')
declared_count=$(printf '%s\n' $DECLARED_NAMES | grep -c . || true)
[ "$declared_count" -ge 10 ] || fail "parsed only $declared_count backbone dependencies from Cargo.toml — the parser is broken, refusing to pass vacuously"
echo "ok: all $declared_count declared backbone dependencies are crates.io requirements (version, no git or path)"

# 3. The committed lockfile resolves unchanged.
METADATA_JSON="$(mktemp "${TMPDIR:-/tmp}/pin-probe-metadata.XXXXXX.json")"
GRAPH_EDGES="$(mktemp "${TMPDIR:-/tmp}/pin-probe-edges.XXXXXX.tsv")"
trap 'rm -f "$METADATA_JSON" "$GRAPH_EDGES"' EXIT
metadata_args=(--locked --format-version 1)
if [ "$MODE" = "offline" ]; then metadata_args+=(--offline); fi
( cd "$REPO_DIR" && cargo metadata "${metadata_args[@]}" >"$METADATA_JSON" ) \
  || fail "cargo metadata --locked did not resolve cleanly"
echo "ok: cargo metadata --locked resolved the full graph"

lock_records() {
  # name|version|source  (source empty for path packages)
  awk '
    /^\[\[package\]\]/ { if (name != "") print name "|" ver "|" src; name=""; ver=""; src=""; next }
    /^name = /    { gsub(/^name = "|"$/, ""); name=$0 }
    /^version = / { gsub(/^version = "|"$/, ""); ver=$0 }
    /^source = /  { gsub(/^source = "|"$/, ""); src=$0 }
    END           { if (name != "") print name "|" ver "|" src }
  ' "$CARGO_LOCK"
}
lock_package_names() { lock_records | cut -d'|' -f1; }
# Membership is tested against this captured list, never by piping the parser
# into `grep -q`: grep exits at the first match, the writer upstream dies of
# SIGPIPE, and under pipefail the whole test reads as "not found".
LOCK_NAMES=$(lock_package_names)
in_lock() { grep -qx -- "$1" <<<"$LOCK_NAMES"; }
lock_name_count=$(lock_package_names | grep -c . || true)
[ "$lock_name_count" -ge 100 ] || fail "parsed only $lock_name_count packages from Cargo.lock — the lock parser is broken, refusing to pass vacuously"

# 4. Backbone packages come from crates.io (path override: dev only, inside metaphora).
# The service's own packages are path-sourced by nature and may carry a backbone-
# name (`backbone-crm-app`), so they are not dependencies to check.
WORKSPACE_NAMES=$(jq -r '.workspace_members[] as $id | .packages[] | select(.id == $id) | .name' "$METADATA_JSON")
backbone_records=$(lock_records | awk -F'|' -v own="$WORKSPACE_NAMES" '
  BEGIN { n = split(own, names, "\n"); for (i = 1; i <= n; i++) skip[names[i]] = 1 }
  $1 ~ /^backbone-/ && !($1 in skip)' || true)
backbone_count=$(printf '%s\n' "$backbone_records" | grep -c . || true)
[ "$backbone_count" -ge "$declared_count" ] || fail "only $backbone_count backbone packages in Cargo.lock for $declared_count declared — the lock is incomplete"
while IFS='|' read -r name ver src; do
  [ -n "$name" ] || continue
  if [ "$src" = "$CRATES_IO" ]; then continue; fi
  if [ -z "$src" ]; then
    [ "$RELEASE" = "false" ] || fail "$name $ver resolves through a local path override — release mode admits crates.io packages only"
    dir=$(jq -r --arg n "$name" '.packages[] | select(.name == $n) | .manifest_path' "$METADATA_JSON" | head -1 | xargs dirname 2>/dev/null || true)
    [ -n "$METAPHORA_DIR" ] || fail "$name is path-sourced but the metaphora checkout was not found beside the workspace"
    case "$(cd "$dir" 2>/dev/null && pwd)/" in
      "$METAPHORA_DIR"/*) echo "DEP-PIN-KIND: $name=path(dev) — local [patch.crates-io] override at $dir; the release posture is crates.io" ;;
      *) fail "$name is path-sourced outside the metaphora checkout ($dir)" ;;
    esac
    continue
  fi
  fail "$name $ver comes from '$src' — backbone crates must come from crates.io"
done <<< "$backbone_records"
echo "ok: all $backbone_count backbone packages in Cargo.lock come from crates.io$( [ "$RELEASE" = "true" ] && echo " (release posture)")"

# 5. Global per-name single-resolve.
global_dupes=$(lock_package_names | grep -E '^backbone-' | sort | uniq -c | awk '$1 != 1 { print $2 "=" $1 }')
if [ -n "$global_dupes" ]; then
  echo "$global_dupes" | sed 's/^/FAIL-global-single-resolve: /' >&2
  fail "a backbone crate resolves more than once in the lock — each backbone name must resolve exactly once"
fi
echo "ok: every backbone-* name resolves exactly once across all $lock_name_count lock packages"

# 6. Every declared backbone dependency is in the lock.
for n in $DECLARED_NAMES; do
  in_lock "$n" || fail "declared dependency $n is missing from Cargo.lock"
done
echo "ok: all $declared_count declared backbone dependencies are present in the lock"

# 7. One framework version across the graph.
fw_versions=$(lock_records | awk -F'|' -v list="$FRAMEWORK_CRATES" 'BEGIN{split(list,a," "); for(i in a) fw[a[i]]=1} fw[$1]{print $2}' | sort -u)
fw_count=$(printf '%s\n' "$fw_versions" | grep -c . || true)
[ "$fw_count" -ge 1 ] || fail "no framework crates found in Cargo.lock — the scan is broken, refusing to pass vacuously"
[ "$fw_count" -eq 1 ] || fail "framework crates resolve to more than one version: $(echo $fw_versions)"
echo "ok: every framework crate in the lock is at $fw_versions"


# ---------------------------------------------------------------------------
# 8. Online: no locked backbone version is yanked on crates.io.
# ---------------------------------------------------------------------------
if [ "$MODE" = "online" ]; then
  yanked=""
  while IFS='|' read -r name ver src; do
    [ "$src" = "$CRATES_IO" ] || continue
    n=${#name}; lower=$(printf '%s' "$name" | tr 'A-Z' 'a-z')
    case $n in 1) p="1/$lower";; 2) p="2/$lower";; 3) p="3/${lower:0:1}/$lower";; *) p="${lower:0:2}/${lower:2:2}/$lower";; esac
    entry=$(curl -fsS -A "backbone-pin-probe" "https://index.crates.io/$p" | jq -cs --arg v "$ver" 'map(select(.vers == $v)) | first // empty') \
      || fail "could not read the crates.io index entry for $name"
    [ -n "$entry" ] || fail "$name $ver is not in the crates.io index"
    [ "$(printf '%s' "$entry" | jq -r .yanked)" = "false" ] || yanked="$yanked $name@$ver"
  done <<< "$backbone_records"
  [ -z "$yanked" ] || fail "locked backbone versions are yanked on crates.io:$yanked — run cargo update for them"
  echo "ok: none of the $backbone_count locked backbone versions is yanked on crates.io"
else
  echo "ok: crates.io yank check skipped offline"
fi
echo "PIN PROBE PASS"
