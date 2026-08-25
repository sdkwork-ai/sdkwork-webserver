#!/usr/bin/env bash
# Report which placeholder modules have a Windows seed dist to sync.
set -euo pipefail
SEED="${1:-/mnt/e/sdkwork-space}"
CHECKOUT="${2:-/opt/deploy/sdkwork-space}"
PLACEHOLDERS=(
  sdkwork-account sdkwork-agentstudio sdkwork-aiot sdkwork-appbase sdkwork-assets
  sdkwork-audio sdkwork-birdcoder2 sdkwork-browser sdkwork-canvas sdkwork-cms
  sdkwork-codebox sdkwork-company sdkwork-dezhou sdkwork-documents sdkwork-feeds
  sdkwork-gameengine sdkwork-github sdkwork-image sdkwork-inventory sdkwork-invoice
  sdkwork-kernel sdkwork-llm sdkwork-local-router sdkwork-mahjong sdkwork-mail
  sdkwork-merchandise sdkwork-modelkit sdkwork-notes sdkwork-portal sdkwork-promotion
  sdkwork-prompts sdkwork-search sdkwork-settings sdkwork-shop sdkwork-tts
  sdkwork-video-cut sdkwork-xiangqi sdkwork-iam sdkwork-voice sdkwork-api-cloud-gateway
)

for mod in "${PLACEHOLDERS[@]}"; do
  win=""
  opt=""
  if [ -d "${SEED}/${mod}/apps" ]; then
    win="$(find "${SEED}/${mod}/apps" -maxdepth 4 \( -path '*/dist/*/index.html' -o -path '*/dist/index.html' \) 2>/dev/null | head -5 | tr '\n' ' ')"
  fi
  if [ -d "${CHECKOUT}/${mod}/apps" ]; then
    opt="$(find "${CHECKOUT}/${mod}/apps" -maxdepth 4 \( -path '*/dist/*/index.html' -o -path '*/dist/index.html' \) 2>/dev/null | head -5 | tr '\n' ' ')"
  fi
  static_src="${CHECKOUT}/${mod}/deployments/webserver/static/index.html"
  static_flag="no-static"
  [ -f "${static_src}" ] && static_flag="has-static"
  if [ -n "${win}" ]; then
    echo "SEED ${mod} (${static_flag}) :: ${win}"
  elif [ -n "${opt}" ]; then
    echo "OPT  ${mod} (${static_flag}) :: ${opt}"
  else
    echo "NONE ${mod} (${static_flag})"
  fi
done
