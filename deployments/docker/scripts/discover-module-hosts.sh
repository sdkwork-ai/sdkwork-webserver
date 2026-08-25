#!/usr/bin/env bash
# Discover module ingress hostnames from nginx sidecars in the space checkout.
# Used by install-wsl-hosts.sh / Windows hosts setup (host nginx retired).
# Authority: SDKWORK_WEBSERVER_SPEC.md §17 (W26 multi-base-domain), APP_RUNTIME_TOPOLOGY_NAMING.md §9
set -euo pipefail

CHECKOUT="${SDKWORK_SPACE_CHECKOUT_HOST_PATH:-${SDKWORK_SPACE_HOST_PATH:-/opt/deploy}/sdkwork-space}"
ENVIRONMENT="${1:-}"
BASE_DOMAIN="${SDKWORK_WSL_BASE_DOMAIN:-sdkwork.com}"
# Default: every registered brand in server_name (im-dev.birdcoder.com, …).
# Set SDKWORK_DISCOVER_BASE_DOMAIN_ONLY=true to limit output to *.${BASE_DOMAIN}.
DISCOVER_BASE_DOMAIN_ONLY="${SDKWORK_DISCOVER_BASE_DOMAIN_ONLY:-false}"

if [ ! -d "${CHECKOUT}" ]; then
  exit 0
fi

case "${ENVIRONMENT}" in
  development|test|staging|production|"") ;;
  *)
    echo "usage: $0 [development|test|staging|production]" >&2
    exit 2
    ;;
esac

# Collect first-label hosts from nginx sidecars (preferred) and environment TOML.
hosts="$(
  {
    for conf in "${CHECKOUT}"/sdkwork-*/deployments/webserver/nginx.standalone.${ENVIRONMENT:-development}.conf; do
      [ -f "${conf}" ] || continue
      case "${conf}" in
        */sdkwork-webserver/*) continue ;;
      esac
      sed -n 's/.*server_name[[:space:]]\+\([^;]*\);.*/\1/p' "${conf}" | tr ' ' '\n'
    done
    if [ -z "${ENVIRONMENT}" ]; then
      for conf in "${CHECKOUT}"/sdkwork-*/deployments/webserver/nginx.standalone.*.conf; do
        [ -f "${conf}" ] || continue
        case "${conf}" in
          */sdkwork-webserver/*) continue ;;
        esac
        sed -n 's/.*server_name[[:space:]]\+\([^;]*\);.*/\1/p' "${conf}" | tr ' ' '\n'
      done
    fi
  } | sed '/^$/d' | sort -u
)"

host_matches_environment() {
  local host="$1"
  case "${ENVIRONMENT}" in
    development)
      case "${host}" in *-dev.*) return 0 ;; *) return 1 ;; esac
      ;;
    test)
      case "${host}" in *-test.*) return 0 ;; *) return 1 ;; esac
      ;;
    staging)
      case "${host}" in *-staging.*) return 0 ;; *) return 1 ;; esac
      ;;
    production)
      case "${host}" in
        *-dev.*|*-test.*|*-staging.*) return 1 ;;
        *) return 0 ;;
      esac
      ;;
    "")
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

printf '%s\n' "${hosts}" | while IFS= read -r host; do
  [ -n "${host}" ] || continue
  if [ -n "${ENVIRONMENT}" ] && ! host_matches_environment "${host}"; then
    continue
  fi
  if [ "${DISCOVER_BASE_DOMAIN_ONLY}" = "true" ]; then
    case "${host}" in
      *"${BASE_DOMAIN}") printf '%s\n' "${host}" ;;
    esac
  else
    printf '%s\n' "${host}"
  fi
done | sort -u
