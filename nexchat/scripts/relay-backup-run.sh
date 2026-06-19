#!/usr/bin/env bash
# EN: Source relay-backup.env and run relay-backup-to-ipfs.sh.
# CN: 加载 relay-backup.env 并执行备份。

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${RELAY_BACKUP_ENV:-$ROOT/relay-ops/relay-backup.env}"

if [[ ! -f "$ENV_FILE" ]]; then
  printf '[relay-backup-run] missing %s\n' "$ENV_FILE" >&2
  printf 'Run: ./scripts/relay-ops-init.sh\n' >&2
  exit 1
fi

# shellcheck disable=SC1090
set -a
source "$ENV_FILE"
set +a

exec "$ROOT/scripts/relay-backup-to-ipfs.sh" "$@"
