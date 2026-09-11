#!/usr/bin/env bash
# build-apps-static.sh — build the Adaptive Web static dist (PC / H5) of any
# independent sibling module in the sdkwork workspace, via the canonical
# browser runner. Implementation: bin/lib/apps-static.sh.
#
# CRLF self-heal: a `core.autocrlf=true` checkout turns bin/lib/*.sh into
# CRLF. MSYS (Git Bash) tolerates that silently, but WSL/Linux bash rejects
# the sourced files with `$'\r': command not found`. Strip CR from the whole
# bootstrap chain before loading it, so the script builds from any directory
# in any shell context (Git Bash, WSL native, Linux).
sdkwork_apps_static_strip_crlf() {
  local bin_dir f cr specs module_root
  bin_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  module_root="$(cd "${bin_dir}/.." && pwd)"
  if [[ -f "${module_root}/../sdkwork-specs/bin/lib/sdkwork-common.sh" ]]; then
    specs="$(cd "${module_root}/../sdkwork-specs" && pwd)"
  elif [[ -f "${module_root}/sdkwork-specs/bin/lib/sdkwork-common.sh" ]]; then
    specs="$(cd "${module_root}/sdkwork-specs" && pwd)"
  else
    return 0
  fi
  for f in "${bin_dir}/lib/apps-static.sh" \
           "${bin_dir}/lib/bootstrap.sh" \
           "${bin_dir}/lib/module.sh" \
           "${specs}/bin/lib/sdkwork-common.sh" \
           "${specs}/bin/lib/entrypoints.sh" \
           "${specs}/bin/lib/ops-config.sh" \
           "${specs}/bin/lib/ops-observe.sh" \
           "${specs}/bin/lib/ops-backup.sh"; do
    [[ -f "${f}" ]] || continue
    cr="$(tr -cd '\r' < "${f}" | wc -c)"
    if (( cr > 0 )); then
      awk '{ sub(/\r$/, ""); print }' "${f}" > "${f}.tmp" && mv "${f}.tmp" "${f}"
      printf '[sdkwork-bin] WARN: stripped CRLF from %s (scripts must stay LF; see .gitattributes)\n' "${f}" >&2
    fi
  done
}
sdkwork_apps_static_strip_crlf

SDKWORK_ENTRY="apps-static"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/apps-static.sh"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/bootstrap.sh"
