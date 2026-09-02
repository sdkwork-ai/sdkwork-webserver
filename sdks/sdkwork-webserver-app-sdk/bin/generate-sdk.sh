#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FAMILY_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
WEB_ROOT="$(cd "${FAMILY_ROOT}/../.." && pwd)"
WORKSPACE_ROOT="$(cd "${FAMILY_ROOT}/../../.." && pwd)"
GENERATOR_PATH="${WORKSPACE_ROOT}/sdkwork-sdk-generator/bin/sdkgen.js"
INPUT_PATH="${FAMILY_ROOT}/openapi/sdkwork-webserver-app-api.sdkgen.yaml"
SDK_NAME="sdkwork-webserver-app-sdk"
BASE_URL="${BASE_URL:-http://localhost:3800}"
SDK_VERSION="${SDK_VERSION:-1.0.0}"
API_PREFIX="/app/v3/api"
LANGUAGES="${LANGUAGES:-typescript,dart,python,go,java,kotlin,swift,csharp,flutter,rust,php,ruby}"

if [[ ! -f "${GENERATOR_PATH}" ]]; then
  echo "Canonical SDK generator not found: ${GENERATOR_PATH}" >&2
  exit 1
fi

if [[ ! -f "${INPUT_PATH}" ]]; then
  node "${WEB_ROOT}/tools/materialize_web_phase1_contracts.mjs"
fi

if [[ ! -f "${INPUT_PATH}" ]]; then
  echo "OpenAPI sdkgen input not found: ${INPUT_PATH}" >&2
  exit 1
fi

package_name() {
  case "$1" in
    typescript) echo "@sdkwork/webserver-app-sdk" ;;
    dart) echo "sdkwork_webserver_app_sdk" ;;
    python) echo "sdkwork-webserver-app-sdk" ;;
    go) echo "github.com/sdkwork/sdkwork-webserver-app-sdk" ;;
    java) echo "com.sdkwork:sdkwork-webserver-app-sdk" ;;
    kotlin) echo "com.sdkwork:sdkwork-webserver-app-sdk" ;;
    swift) echo "sdkwork-webserver-app-sdk" ;;
    csharp) echo "SDKWork.WebserverAppSdk" ;;
    flutter) echo "sdkwork_webserver_app_sdk" ;;
    rust) echo "sdkwork-webserver-app-sdk" ;;
    php) echo "sdkwork/webserver-app-sdk" ;;
    ruby) echo "sdkwork-webserver-app-sdk" ;;
    *) echo "Unsupported SDK language: $1" >&2; return 1 ;;
  esac
}

namespace_args() {
  case "$1" in
    java) printf '%s\n' "--namespace" "com.sdkwork.webserver.app.sdk" ;;
    kotlin) printf '%s\n' "--namespace" "com.sdkwork.webserver.app.sdk" ;;
    csharp) printf '%s\n' "--namespace" "SDKWork.WebserverAppSdk" ;;
    php) printf '%s\n' "--namespace" "SDKWork\Webserver\AppSdk" ;;
  esac
}

IFS=',' read -r -a language_array <<< "${LANGUAGES}"
for language in "${language_array[@]}"; do
  language="$(echo "${language}" | xargs)"
  [[ -z "${language}" ]] && continue
  output_path="${FAMILY_ROOT}/${SDK_NAME}-${language}/generated/server-openapi"
  mapfile -t ns_args < <(namespace_args "${language}")
  node "${GENERATOR_PATH}" generate \
    -i "${INPUT_PATH}" \
    -o "${output_path}" \
    -n "${SDK_NAME}" \
    -t app \
    -l "${language}" \
    --fixed-sdk-version "${SDK_VERSION}" \
    --base-url "${BASE_URL}" \
    --api-prefix "${API_PREFIX}" \
    --package-name "$(package_name "${language}")" \
    --standard-profile sdkwork-v3 \
    --sdk-root "${FAMILY_ROOT}" \
    --sdk-name "${SDK_NAME}" \
    --no-sync-published-version \
    "${ns_args[@]}"
done
