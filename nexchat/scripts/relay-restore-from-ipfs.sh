#!/usr/bin/env bash
# EN: Restore RELAY_DATA_DIR from an encrypted IPFS backup (latest manifest or explicit CID).
# CN: 从 IPFS 加密备份恢复 RELAY_DATA_DIR（用 latest 清单或指定 CID）。
#
# Usage / 用法:
#   ./scripts/relay-restore-from-ipfs.sh
#   ./scripts/relay-restore-from-ipfs.sh bafy...
#   BACKUP_LATEST_FILE=... GPG_PASSPHRASE_FILE=... ./scripts/relay-restore-from-ipfs.sh

set -euo pipefail

RELAY_DATA_DIR="${RELAY_DATA_DIR:-${PWD}/data}"
BACKUP_LATEST_FILE="${BACKUP_LATEST_FILE:-$(dirname "$RELAY_DATA_DIR")/latest-backup.json}"
IPFS_API="${IPFS_API:-${IPFS_API_URL:-http://127.0.0.1:5001}}"
GPG_PASSPHRASE_FILE="${GPG_PASSPHRASE_FILE:-}"
AGE_IDENTITY_FILE="${AGE_IDENTITY_FILE:-}"
RELAY_STOP_CMD="${RELAY_STOP_CMD:-}"
RELAY_START_CMD="${RELAY_START_CMD:-}"
RESTORE_DRY_RUN="${RESTORE_DRY_RUN:-0}"

log() { printf '[relay-restore] %s\n' "$*" >&2; }
die() { log "ERROR: $*"; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing command: $1"
}

resolve_cid() {
  if [[ -n "${1:-}" ]]; then
    printf '%s' "$1"
    return 0
  fi
  [[ -f "$BACKUP_LATEST_FILE" ]] || die "no CID arg and missing $BACKUP_LATEST_FILE"
  require_cmd jq
  jq -er .cid "$BACKUP_LATEST_FILE"
}

read_manifest_field() {
  local field="$1"
  require_cmd jq
  jq -er ".${field}" "$BACKUP_LATEST_FILE"
}

ipfs_cat_to() {
  local cid="$1"
  local out="$2"
  if command -v ipfs >/dev/null 2>&1; then
    ipfs cat "$cid" >"$out"
    return 0
  fi
  require_cmd curl
  curl -sfS -X POST "${IPFS_API%/}/api/v0/cat?arg=${cid}" -o "$out"
}

decrypt_archive() {
  local enc="$1"
  local plain="$2"
  local mode="${3:-gpg}"

  case "$mode" in
    age)
      require_cmd age
      [[ -n "$AGE_IDENTITY_FILE" ]] || die "AGE_IDENTITY_FILE required for age restore"
      age -d -i "$AGE_IDENTITY_FILE" -o "$plain" "$enc"
      ;;
    gpg)
      require_cmd gpg
      if [[ -n "$GPG_PASSPHRASE_FILE" ]]; then
        [[ -r "$GPG_PASSPHRASE_FILE" ]] || die "GPG_PASSPHRASE_FILE not readable"
        gpg --batch --yes --decrypt --passphrase-file "$GPG_PASSPHRASE_FILE" -o "$plain" "$enc"
      else
        gpg --batch --yes --decrypt -o "$plain" "$enc"
      fi
      ;;
    none)
      cp "$enc" "$plain"
      ;;
    *)
      die "unsupported encrypted mode: $mode"
      ;;
  esac
}

main() {
  require_cmd tar
  local cid enc_mode plain_dir parent name
  cid="$(resolve_cid "${1:-}")"
  log "restoring cid=$cid"

  if [[ -f "$BACKUP_LATEST_FILE" && -z "${1:-}" ]]; then
    enc_mode="$(read_manifest_field encrypted)"
    log "manifest encrypted=$enc_mode created_at=$(read_manifest_field created_at)"
  else
    enc_mode="${BACKUP_ENCRYPT:-gpg}"
  fi

  local work enc plain
  work="$(mktemp -d)"
  enc="$work/relay-backup.enc"
  plain="$work/relay-data.tgz"

  if [[ -n "$RELAY_STOP_CMD" ]]; then
    log "stopping relay: $RELAY_STOP_CMD"
    bash -c "$RELAY_STOP_CMD"
  fi

  trap '[[ -n "${RELAY_START_CMD:-}" ]] && { log "starting relay: $RELAY_START_CMD"; bash -c "$RELAY_START_CMD"; } || true; rm -rf "$work"' EXIT

  log "fetching from IPFS"
  ipfs_cat_to "$cid" "$enc"

  log "decrypting (mode=$enc_mode)"
  decrypt_archive "$enc" "$plain" "$enc_mode"

  plain_dir="$(dirname "$RELAY_DATA_DIR")"
  name="$(basename "$RELAY_DATA_DIR")"
  mkdir -p "$plain_dir"

  if [[ "$RESTORE_DRY_RUN" == "1" ]]; then
    log "dry-run: would extract into $plain_dir (top-level: $name)"
    tar tzf "$plain" | head -20
    exit 0
  fi

  if [[ -d "$RELAY_DATA_DIR" ]]; then
    local bak="${RELAY_DATA_DIR}.pre-restore.$(date -u +%Y%m%dT%H%M%SZ)"
    log "moving existing data to $bak"
    mv "$RELAY_DATA_DIR" "$bak"
  fi

  log "extracting to $plain_dir"
  tar xzf "$plain" -C "$plain_dir"
  [[ -d "$RELAY_DATA_DIR" ]] || die "archive did not contain $name under $plain_dir"

  log "restore complete → $RELAY_DATA_DIR"
}

main "$@"
