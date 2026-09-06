#!/bin/bash
set -e
d=/opt/deploy/sdkwork-space/sdkwork-env-dispatch/deployments/webserver
for pair in "server.staging.toml:server.demo.toml" "nginx.cloud.staging.conf:nginx.cloud.demo.conf" "nginx.standalone.staging.conf:nginx.standalone.demo.conf"; do
  src="${pair%%:*}"; out="${pair##*:}"
  sed 's/staging/demo/g' "$d/$src" > "$d/$out"
  echo "created $out ($(wc -l < "$d/$out") lines)"
done
