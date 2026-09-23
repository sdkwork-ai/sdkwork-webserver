#!/usr/bin/env bash
# Mutation-test tools/check-webserver-materialization.mjs.
# Discipline: control group must be green, every mutation must be proven to have
# changed the artifact tree (a no-op mutation proves nothing), and the restore
# must return the tree to the recorded control hash.
set -u
cd "$(dirname "$0")/.." || exit 1
ROOT=$PWD
GATE="node tools/check-webserver-materialization.mjs"
trap 'pnpm run --silent align:webserver >/dev/null 2>&1' EXIT

tree_hash() {
  ( cd deployments/webserver && find . -type f -not -path "./static/*" -print0 \
      | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1 )
}

restore() {
  pnpm run --silent align:webserver >/dev/null 2>&1
}

echo "control tree hash: $(tree_hash)"
$GATE >/dev/null 2>&1 && echo "control gate: GREEN (expected)" || { echo "control gate: RED — abort"; exit 1; }

pass=0
fail=0

run_case() {
  local name=$1
  local mutate=$2
  local cleanup=${3:-}
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
  [ -n "$cleanup" ] && eval "$cleanup"
  restore
  if [ "$(tree_hash)" != "$before" ]; then
    echo "  [$name] RESTORE MISMATCH — aborting"
    exit 1
  fi
}

echo "mutation cases:"
run_case "M1 drop non-prod vhost (replays 75c68448)" \
  "sed -i '/^\[\[http.server\]\]/,\$d' deployments/webserver/server.development.toml"
run_case "M2 hand-edit sidecar (client_max_body_size)" \
  "sed -i 's/client_max_body_size 1100m;/client_max_body_size 100m;/' deployments/webserver/nginx.standalone.development.conf"
run_case "M3 stray file" \
  "echo '# stray' > deployments/webserver/extra-hand-edit.toml" \
  "rm -f deployments/webserver/extra-hand-edit.toml"
run_case "M4 weaken exposure-governance snippet" \
  "sed -i 's|location = /metrics {|location = /metrics-moved {|' deployments/webserver/snippets/gateway-locations.production.conf"
run_case "M5 drop demo tier vhost" \
  "sed -i '/^\[\[http.server\]\]/,\$d' deployments/webserver/server.demo.toml"

rm -f deployments/webserver/extra-hand-edit.toml
restore
echo "final tree hash: $(tree_hash)"
restore
$GATE >/dev/null 2>&1 && echo "final gate: GREEN (restored)" || echo "final gate: RED — restore failed"
echo "mutation summary: $pass pass / $fail false-negative"
[ "$fail" -eq 0 ]
