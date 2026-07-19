#!/usr/bin/env bash
# Bootstrap the EVM workspace for the NEX asset bridge contracts.
# Pinned versions match the deployed Hyperbridge host on Polygon.
#
# 初始化 NEX 资产桥 EVM 工作区依赖。版本与 Polygon 上部署的 Hyperbridge host 对齐。
set -euo pipefail

cd "$(dirname "$0")"

if ! command -v forge >/dev/null 2>&1; then
  echo "foundry not found. Install: curl -L https://foundry.paradigm.xyz | bash && foundryup" >&2
  exit 1
fi

forge install openzeppelin/openzeppelin-contracts --no-commit
forge install OpenZeppelin/openzeppelin-contracts-upgradeable --no-commit
# ismp-solidity is the official ISMP Solidity ABI; pin a tag that matches the
# deployed Host when you wire a real network.
forge install polytope-labs/ismp-solidity --no-commit

echo "Done. Run: forge build && forge test"
