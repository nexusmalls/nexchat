#!/usr/bin/env bash
# EN: Encrypt + tarball RELAY_DATA_DIR, upload to IPFS, pin, write latest-backup manifest.
# CN: 打包加密 RELAY_DATA_DIR，上传 IPFS 并 pin，写入 latest-backup 清单。
#
# Requirements / 依赖:
#   tar, sha256sum (or shasum), curl OR ipfs CLI, age (preferred) or gpg
#
# Quick start / 快速开始:
#   export RELAY_DATA_DIR=/opt/nexchat-relay/data
#   export AGE_RECIPIENT=age1...          # or GPG_PASSPHRASE_FILE=/secure/relay-backup.pass
#   export BACKUP_LATEST_FILE=/opt/nexchat-relay/latest-backup.json
#   ./scripts/relay-backup-to-ipfs.sh
#
# Restore (new host) / 换机恢复:
#   CID=$(jq -r .cid /opt/nexchat-relay/latest-backup.json)
#   ipfs cat "$CID" > /tmp/relay-backup.enc
#   age -d -i /secure/relay-backup.key -o /tmp/relay-data.tgz /tmp/relay-backup.enc
#   # gpg: gpg --batch --decrypt -o /tmp/relay-data.tgz /tmp/relay-backup.enc
#   systemctl stop nexchat-relay
#   tar xzf /tmp/relay-data.tgz -C /opt/nexchat-relay
#   systemctl start nexchat-relay
#
# Cron example / 定时示例 (every 15 min, stop relay briefly):
#   */15 * * * * RELAY_DATA_DIR=/opt/nexchat-relay/data AGE_RECIPIENT=age1... \
#     RELAY_STOP_CMD='systemctl stop nexchat-relay' \
#     RELAY_START_CMD='systemctl start nexchat-relay' \
#     /opt/nexchat/scripts/relay-backup-to-ipfs.sh >> /var/log/nexchat-relay-backup.log 2>&1

set -euo pipefail

RELAY_WAS_STOPPED=0
_BACKUP_WORK=""
RELAY_DATA_DIR="${RELAY_DATA_DIR:-${PWD}/data}"
BACKUP_LATEST_FILE="${BACKUP_LATEST_FILE:-$(dirname "$RELAY_DATA_DIR")/latest-backup.json}"
BACKUP_LOCAL_DIR="${BACKUP_LOCAL_DIR:-$(dirname "$RELAY_DATA_DIR")/backups}"
BACKUP_KEEP_LOCAL="${BACKUP_KEEP_LOCAL:-5}"
IPFS_API="${IPFS_API:-${IPFS_API_URL:-http://127.0.0.1:5001}}"
IPFS_PIN="${IPFS_PIN:-1}"
BACKUP_STOP_RELAY="${BACKUP_STOP_RELAY:-1}"
RELAY_STOP_CMD="${RELAY_STOP_CMD:-}"
RELAY_START_CMD="${RELAY_START_CMD:-}"
# age | gpg | auto (prefer age when AGE_RECIPIENT set, else gpg when passphrase file set)
BACKUP_ENCRYPT="${BACKUP_ENCRYPT:-auto}"
AGE_RECIPIENT="${AGE_RECIPIENT:-}"
AGE_IDENTITY_FILE="${AGE_IDENTITY_FILE:-}"
GPG_PASSPHRASE_FILE="${GPG_PASSPHRASE_FILE:-}"
GPG_RECIPIENT="${GPG_RECIPIENT:-}"
# EN: off-host copy of the manifest (ADR CHAT_SYNC_ANCHOR §4.2: if host dies, the latest CID
# must survive elsewhere). Command receives the manifest path as $1, e.g.
#   BACKUP_OFFSITE_CMD='scp "$1" backup@offsite:/srv/relay/latest-backup.json'
# CN: 清单异地存放（ADR CHAT_SYNC_ANCHOR §4.2：主机失联时最新 CID 必须在他处可得）。
# 命令以 $1 接收清单路径，如上例 scp。
BACKUP_OFFSITE_CMD="${BACKUP_OFFSITE_CMD:-}"

log() { printf '[relay-backup] %s\n' "$*" >&2; }
die() { log "ERROR: $*"; exit 1; }

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

iso_now() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

now_ms() {
  printf '%s000' "$(date +%s)" 2>/dev/null || date +%s000
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing command: $1"
}

stop_relay() {
  [[ "$BACKUP_STOP_RELAY" == "1" ]] || return 0
  [[ -n "$RELAY_STOP_CMD" ]] || {
    log "BACKUP_STOP_RELAY=1 but RELAY_STOP_CMD unset; continuing (snapshot may be slightly inconsistent)"
    return 0
  }
  log "stopping relay: $RELAY_STOP_CMD"
  bash -c "$RELAY_STOP_CMD"
  RELAY_WAS_STOPPED=1
}

start_relay() {
  [[ "${RELAY_WAS_STOPPED:-0}" == "1" ]] || return 0
  [[ -n "$RELAY_START_CMD" ]] || return 0
  log "starting relay: $RELAY_START_CMD"
  bash -c "$RELAY_START_CMD"
}

pick_encrypt_mode() {
  local mode="$BACKUP_ENCRYPT"
  if [[ "$mode" == "auto" ]]; then
    if [[ -n "$AGE_RECIPIENT" ]]; then
      mode="age"
    elif [[ -n "$GPG_PASSPHRASE_FILE" || -n "$GPG_RECIPIENT" ]]; then
      mode="gpg"
    else
      die "set AGE_RECIPIENT or GPG_PASSPHRASE_FILE (or BACKUP_ENCRYPT=none for dev only)"
    fi
  fi
  printf '%s' "$mode"
}

encrypt_archive() {
  local plain="$1"
  local out="$2"
  local mode="$3"

  case "$mode" in
    age)
      require_cmd age
      [[ -n "$AGE_RECIPIENT" ]] || die "AGE_RECIPIENT required for age encryption"
      age -r "$AGE_RECIPIENT" -o "$out" "$plain"
      ;;
    gpg)
      require_cmd gpg
      if [[ -n "$GPG_RECIPIENT" ]]; then
        gpg --batch --yes --trust-model always -e -r "$GPG_RECIPIENT" -o "$out" "$plain"
      elif [[ -n "$GPG_PASSPHRASE_FILE" ]]; then
        [[ -r "$GPG_PASSPHRASE_FILE" ]] || die "GPG_PASSPHRASE_FILE not readable"
        gpg --batch --yes --symmetric --cipher-algo AES256 \
          --passphrase-file "$GPG_PASSPHRASE_FILE" -o "$out" "$plain"
      else
        die "set GPG_RECIPIENT or GPG_PASSPHRASE_FILE for gpg encryption"
      fi
      ;;
    none)
      log "WARNING: BACKUP_ENCRYPT=none — uploading plaintext tarball to IPFS (dev only)"
      cp "$plain" "$out"
      ;;
    *)
      die "unknown BACKUP_ENCRYPT=$mode"
      ;;
  esac
}

ipfs_add_pin() {
  local file="$1"
  local cid=""

  if command -v ipfs >/dev/null 2>&1; then
    if [[ "$IPFS_PIN" == "1" ]]; then
      cid="$(ipfs add --quieter --pin=true "$file" | tail -n1)"
    else
      cid="$(ipfs add --quieter "$file" | tail -n1)"
    fi
    printf '%s' "$cid"
    return 0
  fi

  require_cmd curl
  local resp
  resp="$(curl -sfS -X POST -F "file=@${file}" \
    "${IPFS_API%/}/api/v0/add?pin=${IPFS_PIN}&quieter=true")"
  cid="$(printf '%s' "$resp" | tail -n1 | sed -n 's/.*"Hash"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
  [[ -n "$cid" ]] || die "ipfs add via HTTP API failed: $resp"
  printf '%s' "$cid"
}

write_latest_manifest() {
  local cid="$1"
  local enc_file="$2"
  local plain_file="$3"
  local mode="$4"
  local ts="$5"
  local ts_ms="$6"
  local enc_bytes plain_bytes enc_sha

  enc_bytes="$(wc -c < "$enc_file" | tr -d ' ')"
  plain_bytes="$(wc -c < "$plain_file" | tr -d ' ')"
  enc_sha="$(sha256_file "$enc_file")"

  mkdir -p "$(dirname "$BACKUP_LATEST_FILE")"
  umask 077
  cat >"$BACKUP_LATEST_FILE" <<EOF
{
  "v": 1,
  "cid": "${cid}",
  "created_at": "${ts}",
  "created_at_ms": ${ts_ms},
  "relay_data_dir": "${RELAY_DATA_DIR}",
  "encrypted": "${mode}",
  "encrypted_sha256": "${enc_sha}",
  "encrypted_bytes": ${enc_bytes},
  "plain_bytes": ${plain_bytes},
  "ipfs_api": "${IPFS_API}",
  "ipfs_pinned": ${IPFS_PIN}
}
EOF
  chmod 600 "$BACKUP_LATEST_FILE" 2>/dev/null || true
  log "wrote manifest: $BACKUP_LATEST_FILE"
}

offsite_manifest() {
  [[ -n "$BACKUP_OFFSITE_CMD" ]] || {
    log "WARNING: BACKUP_OFFSITE_CMD unset — latest-backup.json lives only on this host (ADR §4.2)"
    return 0
  }
  if bash -c "$BACKUP_OFFSITE_CMD" _ "$BACKUP_LATEST_FILE"; then
    log "manifest copied off-host"
  else
    log "ERROR: off-host manifest copy FAILED — fix before relying on this backup for DR"
    return 1
  fi
}

prune_local_backups() {
  [[ "$BACKUP_KEEP_LOCAL" -gt 0 ]] || return 0
  [[ -d "$BACKUP_LOCAL_DIR" ]] || return 0
  mapfile -t old < <(ls -1t "$BACKUP_LOCAL_DIR"/relay-data-*.enc 2>/dev/null || true)
  local i
  for ((i = BACKUP_KEEP_LOCAL; i < ${#old[@]}; i++)); do
    rm -f "${old[$i]}"
  done
}

backup_cleanup() {
  [[ -n "${_BACKUP_WORK:-}" ]] && rm -rf "$_BACKUP_WORK"
  _BACKUP_WORK=""
  start_relay
}

main() {
  require_cmd tar
  [[ -d "$RELAY_DATA_DIR" ]] || die "RELAY_DATA_DIR not found: $RELAY_DATA_DIR"

  local mode ts ts_ms stamp plain enc cid
  mode="$(pick_encrypt_mode)"
  ts="$(iso_now)"
  ts_ms="$(now_ms)"
  stamp="$(date -u +"%Y%m%dT%H%M%SZ")"
  _BACKUP_WORK="$(mktemp -d)"
  plain="$_BACKUP_WORK/relay-data-${stamp}.tar.gz"
  enc="$BACKUP_LOCAL_DIR/relay-data-${stamp}.enc"

  mkdir -p "$BACKUP_LOCAL_DIR"
  trap backup_cleanup EXIT

  stop_relay

  log "packing $RELAY_DATA_DIR"
  tar czf "$plain" -C "$(dirname "$RELAY_DATA_DIR")" "$(basename "$RELAY_DATA_DIR")"

  log "encrypting (mode=$mode)"
  encrypt_archive "$plain" "$enc" "$mode"

  log "uploading to IPFS (pin=$IPFS_PIN)"
  cid="$(ipfs_add_pin "$enc")"
  [[ -n "$cid" ]] || die "empty CID from ipfs add"

  write_latest_manifest "$cid" "$enc" "$plain" "$mode" "$ts" "$ts_ms"
  offsite_manifest
  prune_local_backups

  log "done cid=$cid enc=$enc"
  printf '%s\n' "$cid"
}

main "$@"
