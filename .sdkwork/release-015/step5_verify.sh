#!/usr/bin/env bash
# 0.1.5 release — step 5: verify all five environments after the rollout.
set -uo pipefail
cd /mnt/e/sdkwork-space/sdkwork-webserver

port_for() {
  case "$1" in
    development) echo 13800 ;;
    test)        echo 18888 ;;
    staging)     echo 18081 ;;
    demo)        echo 19080 ;;
    production)  echo 18080 ;;
    *)           echo "" ;;
  esac
}

echo "=== healthz + image + container health ==="
printf "%-12s %-7s %-6s %-52s %s\n" ENV MGMT HTTP IMAGE HEALTH
for e in development test staging demo production; do
  p="$(port_for "$e")"
  code="$(curl -s -o /dev/null -w '%{http_code}' --noproxy '*' --max-time 10 "http://127.0.0.1:${p}/healthz" || echo ERR)"
  info="$(docker inspect "sdkwork-webserver-${e}-i1-webserver-1" --format '{{.Config.Image}}|{{.State.Health.Status}}' 2>/dev/null)"
  img="${info%%|*}"
  h="${info##*|}"
  printf "%-12s %-7s %-6s %-52s %s\n" "$e" "$p" "$code" "$img" "$h"
done

echo
echo "=== healthz body (development) ==="
curl -s --noproxy '*' --max-time 10 http://127.0.0.1:13800/healthz | head -c 300
echo
echo "=== healthz body (production) ==="
curl -s --noproxy '*' --max-time 10 http://127.0.0.1:18080/healthz | head -c 300
echo
echo "=== gateway + knowledgebase sidecars ==="
docker ps --format '{{.Names}}  {{.Status}}' | grep -E 'webserver-(development|test|staging|demo|production)-gateway' | sort

echo
echo "=== live env files unchanged? (sha256 -c) ==="
( cd /opt/deploy/sdkwork-webserver/bundle && sha256sum -c /var/tmp/webserver-env-pre-0.1.5.sha256 2>&1 )

echo
echo "=== deployed bundle identity ==="
grep -o '"version": "[^"]*"' /opt/deploy/sdkwork-webserver/bundle/manifest.json
cat /opt/deploy/sdkwork-webserver/bundle/image.env
echo "VERIFY_DONE"
