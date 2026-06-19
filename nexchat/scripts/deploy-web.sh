#!/usr/bin/env bash
# EN: Build NexChat for https://nexusmall.net/nexchat/ and rsync dist/ to the VPS.
# CN: 构建 NexChat 并 rsync 到生产 VPS（/nexchat/ 子路径）。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ENV_FILE="$ROOT/deploy/deploy-web.env"
if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  set -a
  source "$ENV_FILE"
  set +a
fi

DEPLOY_SSH="${DEPLOY_SSH:-root@151.158.134.181}"
DEPLOY_REMOTE_PATH="${DEPLOY_REMOTE_PATH:-/var/www/nexchat/}"
DEPLOY_BASE="${DEPLOY_BASE:-/nexchat/}"
DEPLOY_RSYNC_EXCLUDES="${DEPLOY_RSYNC_EXCLUDES:-nexchat.apk}"

if [[ -n "${DEPLOY_SSHPASS:-}" ]]; then
  export SSHPASS="$DEPLOY_SSHPASS"
fi

log() { printf '[deploy-web] %s\n' "$*" >&2; }

if ! command -v rsync >/dev/null 2>&1; then
  log "rsync is required"
  exit 1
fi

RSYNC_SSH=(ssh -o StrictHostKeyChecking=accept-new)
if [[ -n "${SSHPASS:-}" ]]; then
  if ! command -v sshpass >/dev/null 2>&1; then
    log "SSHPASS is set but sshpass is not installed"
    exit 1
  fi
  RSYNC_SSH=(sshpass -e ssh -o StrictHostKeyChecking=accept-new)
fi

ENV_PRODUCTION="$ROOT/.env.production"
if [[ -f "$ENV_PRODUCTION" ]]; then
  if grep -Eq '^[[:space:]]*VITE_USE_MOCK[[:space:]]*=[[:space:]]*true' "$ENV_PRODUCTION"; then
    log "refusing deploy: $ENV_PRODUCTION has VITE_USE_MOCK=true"
    exit 1
  fi
else
  log "warning: $ENV_PRODUCTION missing — ensure CI sets VITE_USE_MOCK=false for production builds"
fi

log "building (mode=production, base=$DEPLOY_BASE)…"
npm run build -- --base="$DEPLOY_BASE"

if [[ ! -d dist ]]; then
  log "dist/ missing after build"
  exit 1
fi

IFS=',' read -r -a EXCLUDE_ARR <<<"$DEPLOY_RSYNC_EXCLUDES"
RSYNC_EXCLUDE_ARGS=()
for x in "${EXCLUDE_ARR[@]}"; do
  x="${x#"${x%%[![:space:]]*}"}"
  x="${x%"${x##*[![:space:]]}"}"
  [[ -z "$x" ]] && continue
  RSYNC_EXCLUDE_ARGS+=(--exclude "$x")
done

REMOTE="${DEPLOY_SSH}:${DEPLOY_REMOTE_PATH}"
log "uploading dist/ → $REMOTE"
rsync -avz --delete "${RSYNC_EXCLUDE_ARGS[@]}" -e "${RSYNC_SSH[*]}" dist/ "$REMOTE"

log "done — https://nexusmall.net/nexchat/"
