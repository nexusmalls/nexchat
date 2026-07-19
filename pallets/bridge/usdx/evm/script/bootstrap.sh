#!/usr/bin/env bash
# Install the exact Solidity dependencies used by the Polygon Amoy USDX lane.
# 安装 Polygon Amoy USDX 通道使用的精确 Solidity 依赖。
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v forge >/dev/null 2>&1; then
  echo "foundry not found. Install: curl -L https://foundry.paradigm.xyz | bash && foundryup" >&2
  exit 1
fi

forge install foundry-rs/forge-std@v1.16.1 --no-commit
forge install OpenZeppelin/openzeppelin-contracts@v5.4.0 --no-commit
forge install polytope-labs/solidity-merkle-trees@12f352fb9b0b311bff26df6a6571329d39ad59be --no-commit
forge install polytope-labs/hyperbridge@3979482228d9001f0463f3192524fa41bc76989b --no-commit

echo "Dependencies pinned. Run: forge build && forge test -vv"
