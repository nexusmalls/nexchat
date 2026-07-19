#!/usr/bin/env bash
# EN: Verify Chopsticks fork (or any Substrate HTTP RPC) after v103 set_code.
# CN: set_code 后验证 fork（或任意 Substrate HTTP RPC）。
set -euo pipefail

RPC="${RPC:-http://127.0.0.1:8000}"

echo "=== RPC: $RPC ==="

SPEC=$(curl -s -H 'Content-Type: application/json' \
  -d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}' \
  "$RPC" | jq -r '.result.specVersion // empty')

if [[ -z "$SPEC" ]]; then
  echo "❌ 无法读取 specVersion（Chopsticks 是否在跑？）"
  exit 1
fi

echo "specVersion: $SPEC"
if [[ "$SPEC" == "103" ]]; then
  echo "✅ specVersion == 103"
else
  echo "⚠️  仍为 $SPEC（未 set_code 或尚未出块）"
fi

echo ""
echo "=== metadata 关键字 ==="
curl -s -H 'Content-Type: application/json' \
  -d '{"id":2,"jsonrpc":"2.0","method":"state_getMetadata","params":[]}' \
  "$RPC" | python3 -c "
import json, sys
hexdata = json.load(sys.stdin).get('result')
if not hexdata:
    print('❌ 无法读取 metadata'); sys.exit(1)
raw = bytes.fromhex(hexdata[2:] if hexdata.startswith('0x') else hexdata)
text = raw.decode('utf-8', errors='replace')
for k in ['MsgIdentity', 'register_device', 'set_opk_root', 'set_stack_caps']:
    print(f\"  {k}: {'✅' if k in text else '❌'}\")
"
