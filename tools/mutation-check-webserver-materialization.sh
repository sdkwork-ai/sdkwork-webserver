#!/usr/bin/env bash
# Mutation-test tools/check-webserver-materialization.mjs.
# Discipline: control group must be green, every mutation must be proven to have
# changed the artifact tree (a no-op mutation proves nothing), and the restore
# must return the tree to the recorded control hash.
# Portability (check-shell-portability): no GNU-only `sha256sum -z` chains and
# no `sed -i`; mutations are POSIX awk with tmp+mv, hashing resolves
# sha256sum -> shasum -> openssl once.
set -u
cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
GATE="node tools/check-webserver-materialization.mjs"
trap 'pnpm run --silent align:webserver >/dev/null 2>&1' EXIT

# Hashing shim: GNU coreutils vs BSD/macOS tooling, resolved once. The
# `PORTABILITY:allow` marker is the documented exemption for a shim that
# probes tool availability first (see check-shell-portability.mjs).
if command -v sha256sum >/dev/null 2>&1; then  # PORTABILITY:allow
  sha256_file() { sha256sum "$@"; }            # PORTABILITY:allow
elif command -v shasum >/dev/null 2>&1; then
  sha256_file() { shasum -a 256 "$@"; }
else
  sha256_file() { openssl dgst -sha256 -r "$@"; }
fi

tree_hash() {
  ( cd deployments/webserver && find . -type f -not -path "./static/*" -print0 \
      | sort -z | xargs -0 sha256_file | sha256_file | cut -d' ' -f1 )
}

restore() {
  pnpm run --silent align:webserver >/dev/null 2>&1
}

# Mutation helpers. Each rewrites its target through a tmp file (BSD/GNU safe).

# Delete from the first `[[http.server]]` table to EOF.
drop_http_server_block() {
  awk 'NR == FNR { if (keep == "" && $0 ~ /^\[\[http\.server\]\]/) keep = NR - 1; next }
       FNR <= keep { print }' "$1" "$1" > "$1.tmp" && mv "$1.tmp" "$1"
}

# Replace the first regex match on every line (POSIX awk `sub`).
substitute_line() {
  awk -v pat="$2" -v rep="$3" '{ sub(pat, rep) } { print }' "$1" > "$1.tmp" \
    && mv "$1.tmp" "$1"
}

echo "control tree hash: $(tree_hash)"
$GATE >/dev/null 2>&1 && echo "control gate: GREEN (expected)" || { echo "control gate: RED — abort"; exit 1; }

pass=0
fail=0

run_case() {
  local name=$1
  local mutate=$2
  local before after gate_rc
  before=$(tree_hash)
  eval "$mutate"
  after=$(tree_hash)
  if [ "$before" = "$after" ]; then
    echo "  [$name] NO-OP mutation (artifact tree unchanged) — inconclusive"
    fail=$((fail + 1))
    return
  fi
  $GATE >/dev/null 2>&1
  gate_rc=$?
  if [ "$gate_rc" -ne 0 ]; then
    echo "  [$name] mutation applied, gate RED -> PASS (message below)"
    $GATE 2>&1 | grep -m1 "^error:" | sed 's/^/      /'
    pass=$((pass + 1))
  else
    echo "  [$name] mutation applied but gate stayed GREEN -> FALSE NEGATIVE"
    fail=$((fail + 1))
  fi
  restore
  if [ "$(tree_hash)" != "$before" ]; then
    echo "  [$name] RESTORE MISMATCH — aborting"
    exit 1
  fi
}

echo "mutation cases:"
run_case "M1 drop non-prod vhost (replays 75c68448)" \
  "drop_http_server_block deployments/webserver/server.development.toml"
run_case "M2 hand-edit sidecar (client_max_body_size)" \
  "substitute_line deployments/webserver/nginx.standalone.development.conf 'client_max_body_size 1100m;' 'client_max_body_size 100m;'"
run_case "M3 stray file" \
  "echo '# stray' > deployments/webserver/extra-hand-edit.toml"
run_case "M4 weaken exposure-governance snippet" \
  "substitute_line deployments/webserver/snippets/gateway-locations.production.conf 'location = /metrics {' 'location = /metrics-moved {'"
run_case "M5 drop demo tier vhost" \
  "drop_http_server_block deployments/webserver/server.demo.toml"

rm -f deployments/webserver/extra-hand-edit.toml
restore
echo "final tree hash: $(tree_hash)"
restore
$GATE >/dev/null 2>&1 && echo "final gate: GREEN (restored)" || echo "final gate: RED — restore failed"
echo "mutation summary: $pass pass / $fail false-negative"
[ "$fail" -eq 0 ]
