#!/usr/bin/env bash
# List crate package/bin names and local debug binaries (portable; no host path).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

echo "=== All crate binary names declared in Cargo.toml ==="
shopt -s nullglob
for f in "${ROOT}/crates/"*/Cargo.toml; do
  crate_dir="$(dirname "$f")"
  crate_name="$(basename "$crate_dir")"
  pkg="$(grep '^name = ' "$f" | head -1 || true)"
  bin="$(grep -A2 '\[\[bin\]\]' "$f" 2>/dev/null | grep 'name =' | head -1 || true)"
  echo "[$crate_name]"
  echo "  package: $pkg"
  [ -n "${bin}" ] && echo "  binary:  $bin"
done

echo ""
echo "=== Actual debug binaries ==="
if compgen -G "${ROOT}/target/debug/sdkwork-*" > /dev/null 2>&1; then
  ls -la "${ROOT}/target/debug"/sdkwork-* 2>/dev/null || true
else
  echo "none"
fi
