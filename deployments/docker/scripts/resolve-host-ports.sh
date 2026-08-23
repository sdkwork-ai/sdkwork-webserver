#!/usr/bin/env bash
# Resolve host-published HTTP/HTTPS ports for WSL external Docker stacks.
# Defaults avoid Ubuntu-native service conflicts (5432/6379/80 stay on host;
# webserver uses 13800/18888/18080 and per-env HTTPS 18430/28430/38430).
set -euo pipefail

host_http_port_for() {
  case "$1" in
    development) echo "${SDKWORK_WEBSERVER_DEV_HOST_PORT:-13800}" ;;
    test) echo "${SDKWORK_WEBSERVER_TEST_HOST_PORT:-18888}" ;;
    production) echo "${SDKWORK_WEBSERVER_PROD_HOST_PORT:-18080}" ;;
    *) return 1 ;;
  esac
}

host_https_port_for() {
  case "$1" in
    development) echo "${SDKWORK_WEBSERVER_DEV_HTTPS_HOST_PORT:-18430}" ;;
    test) echo "${SDKWORK_WEBSERVER_TEST_HTTPS_HOST_PORT:-28430}" ;;
    production) echo "${SDKWORK_WEBSERVER_PROD_HTTPS_HOST_PORT:-38430}" ;;
    *) return 1 ;;
  esac
}

domain_for() {
  case "$1" in
    development) echo server-dev.sdkwork.com ;;
    test) echo server-test.sdkwork.com ;;
    production) echo server.sdkwork.com ;;
    *) return 1 ;;
  esac
}

load_host_ports_from_env() {
  local env_file="$1"
  if [ ! -f "${env_file}" ]; then
    return 0
  fi
  set -a
  # shellcheck disable=SC1090
  eval "$(grep -E '^SDKWORK_WEBSERVER_(DEV|TEST|PROD)_(HOST|HTTPS_HOST)_PORT=' "${env_file}" || true)"
  set +a
}
