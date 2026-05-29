#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

if [[ -z "${PAYOUT_URI:-${PAYOUT_MNEMONIC:-}}" ]]; then
  echo "PAYOUT_URI 或 PAYOUT_MNEMONIC 未设置，无法执行 staking payout。" >&2
  exit 1
fi

MAX_TXS="${PAYOUT_MAX_TXS:-50}"
SUMMARY_DIR="${PAYOUT_SUMMARY_DIR:-$ROOT_DIR/logs/staking-payout}"
RUN_ID="$(date +%Y%m%dT%H%M%S%z)"
RUN_DIR="$SUMMARY_DIR/$RUN_ID"
mkdir -p "$RUN_DIR"

log() {
  printf '== [staking-payout] %s %s ==\n' "$1" "$(date -Is)"
}

run_and_capture_summary() {
  local name="$1"
  shift
  local out_file="$RUN_DIR/${name}.log"
  local summary_file="$RUN_DIR/${name}.summary.json"

  "$@" | tee "$out_file"

  local summary_line
  summary_line="$(tail -n 1 "$out_file")"
  if [[ "$summary_line" == \{"type":"staking-payout-summary"* ]]; then
    printf '%s\n' "$summary_line" > "$summary_file"
  else
    echo "未在 ${name} 输出末尾找到 staking-payout-summary JSON。" >&2
    return 1
  fi
}

log start
node --import tsx e2e/mytests/validator-staking-audit.ts --event-blocks 0 | tee "$RUN_DIR/pre-audit.log"
run_and_capture_summary dry-run node --import tsx e2e/mytests/validator-staking-payout.ts --dry-run --max "$MAX_TXS"
run_and_capture_summary execute node --import tsx e2e/mytests/validator-staking-payout.ts --yes --max "$MAX_TXS"
node --import tsx e2e/mytests/validator-staking-audit.ts --event-blocks 0 | tee "$RUN_DIR/post-audit.log"
cp "$RUN_DIR/execute.summary.json" "$SUMMARY_DIR/latest.summary.json"
log done
