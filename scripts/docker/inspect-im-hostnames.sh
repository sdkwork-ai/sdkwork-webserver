#!/usr/bin/env bash
set -euo pipefail

for c in sdkwork-webserver-development sdkwork-webserver-test sdkwork-webserver-production; do
  echo "=== $c ==="
  docker exec "$c" sh -c 'ls /etc/sdkwork/webserver/modules/sdkwork-im/; grep -nE "server_name|listen " /etc/sdkwork/webserver/modules/sdkwork-im/nginx.standalone.*.conf | head -30'
done

echo '=== host checkout im confs ==='
for f in /opt/deploy/sdkwork-space/sdkwork-im/deployments/webserver/nginx.standalone.*.conf; do
  echo "-- $f"
  grep -E 'server_name|listen ' "$f" | head -12
done

echo '=== env check ==='
for c in sdkwork-webserver-development sdkwork-webserver-test sdkwork-webserver-production; do
  echo -n "$c: "
  docker exec "$c" printenv SDKWORK_WEBSERVER_ENVIRONMENT || true
done
