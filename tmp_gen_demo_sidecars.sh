#!/bin/bash
set -e
d=/opt/deploy/sdkwork-space/sdkwork-api-cloud-gateway/deployments/webserver
for f in server.staging.toml nginx.cloud.staging.conf nginx.standalone.staging.conf; do
  case "$f" in
    server.staging.toml) out="server.demo.toml" ;;
    nginx.cloud.staging.conf) out="nginx.cloud.demo.conf" ;;
    nginx.standalone.staging.conf) out="nginx.standalone.demo.conf" ;;
  esac
  sed 's/staging/demo/g' "$d/$f" > "$d/$out"
  echo "created $d/$out ($(wc -l < "$d/$out") lines)"
done
grep -c "api-demo" "$d/server.demo.toml" "$d/nginx.cloud.demo.conf" "$d/nginx.standalone.demo.conf"
