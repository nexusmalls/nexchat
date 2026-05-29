#!/usr/bin/env bash
#
# generate-deployment-accounts.sh
#
# 根据 docs/部署.md 生成主网部署所需的全部账户密钥，输出到 JSON 文件。
# 所有 SS58 地址使用 Nexus 链 prefix 273（X 开头）。
#
# 生成内容：
#   - 1 个创始者/Sudo 账户（新生成 Sr25519）
#   - 3 个验证者（各含 Sr25519 + Ed25519 密钥对）
#   - 3 个委员会成员（新生成 Sr25519）
#
# 依赖：nexus-node, jq, node (with @polkadot/util-crypto), python3
#
# 用法：
#   chmod +x mytests/generate-deployment-accounts.sh
#   ./mytests/generate-deployment-accounts.sh [输出文件路径]
#
# 默认输出：mytests/secrets/deployment-accounts.json
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
SCRIPTS_DIR="$REPO_ROOT/scripts"
NODE_BIN="$REPO_ROOT/target/release/nexus-node"
OUTPUT="${1:-$SCRIPT_DIR/secrets/deployment-accounts.json}"
SS58_PREFIX=273

mkdir -p "$(dirname "$OUTPUT")"

if [[ ! -x "$NODE_BIN" ]]; then
  echo "ERROR: nexus-node binary not found at $NODE_BIN" >&2
  echo "       Run: cargo build --release" >&2
  exit 1
fi
command -v jq &>/dev/null || { echo "ERROR: jq is required" >&2; exit 1; }
command -v node &>/dev/null || { echo "ERROR: node is required" >&2; exit 1; }
command -v python3 &>/dev/null || { echo "ERROR: python3 is required (for big-integer arithmetic)" >&2; exit 1; }

# 验证 @polkadot/util-crypto 可用
node -e "require('@polkadot/util-crypto')" --preserve-symlinks-main 2>/dev/null \
  || (cd "$SCRIPTS_DIR" && node -e "require('@polkadot/util-crypto')" 2>/dev/null) \
  || { echo "ERROR: @polkadot/util-crypto not found. Run: cd scripts && npm install" >&2; exit 1; }

echo "=== Nexus 主网部署账户生成器 ===" >&2
echo "Binary: $NODE_BIN" >&2
echo "Output: $OUTPUT" >&2
echo "SS58 Prefix: $SS58_PREFIX (X 开头地址)" >&2
echo "" >&2

# ─── SS58 编码：公钥 hex → Nexus 地址 (prefix 273) ───

ss58_encode() {
  local hex_pubkey="$1"
  cd "$SCRIPTS_DIR" && node -e "
    const { encodeAddress } = require('@polkadot/util-crypto');
    console.log(encodeAddress('${hex_pubkey}', ${SS58_PREFIX}));
  " 2>/dev/null
}

# ─── 辅助函数：从 nexus-node key 输出中提取字段 ───

extract_value() {
  local output="$1"
  local label="$2"
  echo "$output" | grep "$label:" | head -1 | sed "s/^.*$label:[[:space:]]*//"
}

# ─── 生成验证者密钥（Sr25519 + Ed25519） ───

generate_validator() {
  local index=$1
  echo ">>> 生成验证者 $index 密钥..." >&2

  local sr_output
  sr_output=$("$NODE_BIN" key generate --scheme Sr25519 2>&1)

  local mnemonic sr_seed sr_pubkey sr_account_id
  mnemonic=$(extract_value "$sr_output" "Secret phrase")
  sr_seed=$(extract_value "$sr_output" "Secret seed")
  sr_pubkey=$(extract_value "$sr_output" "Public key (hex)")
  sr_account_id=$(extract_value "$sr_output" "Account ID")

  # SS58 编码为 Nexus 地址
  local sr_ss58
  sr_ss58=$(ss58_encode "$sr_pubkey")

  local ed_output ed_pubkey ed_ss58
  ed_output=$("$NODE_BIN" key inspect --scheme Ed25519 "$mnemonic" 2>&1)
  ed_pubkey=$(extract_value "$ed_output" "Public key (hex)")
  ed_ss58=$(ss58_encode "$ed_pubkey")

  jq -n \
    --arg name "Validator-${index}" \
    --arg mnemonic "$mnemonic" \
    --arg sr_seed "$sr_seed" \
    --arg sr_pubkey "$sr_pubkey" \
    --arg sr_account_id "$sr_account_id" \
    --arg sr_ss58 "$sr_ss58" \
    --arg ed_pubkey "$ed_pubkey" \
    --arg ed_ss58 "$ed_ss58" \
    '{
      name: $name,
      mnemonic: $mnemonic,
      sr25519: {
        secret_seed: $sr_seed,
        public_key: $sr_pubkey,
        account_id: $sr_account_id,
        ss58_address: $sr_ss58
      },
      ed25519: {
        public_key: $ed_pubkey,
        ss58_address: $ed_ss58
      },
      staking_bond: "10000000000000000",
      staking_bond_human: "10,000 NEX",
      roles: ["validator", "babe", "grandpa"]
    }'
}

# ─── 生成委员会成员密钥（Sr25519） ───

generate_committee_member() {
  local index=$1
  local is_prime=$2
  local name
  if [[ "$is_prime" == "true" ]]; then
    name="Prime (委员会主席)"
  else
    name="Member${index}"
  fi
  echo ">>> 生成委员会成员 $index ($name) 密钥..." >&2

  local sr_output
  sr_output=$("$NODE_BIN" key generate --scheme Sr25519 2>&1)

  local mnemonic sr_seed sr_pubkey sr_account_id sr_ss58
  mnemonic=$(extract_value "$sr_output" "Secret phrase")
  sr_seed=$(extract_value "$sr_output" "Secret seed")
  sr_pubkey=$(extract_value "$sr_output" "Public key (hex)")
  sr_account_id=$(extract_value "$sr_output" "Account ID")
  sr_ss58=$(ss58_encode "$sr_pubkey")

  jq -n \
    --arg name "$name" \
    --arg mnemonic "$mnemonic" \
    --arg sr_seed "$sr_seed" \
    --arg sr_pubkey "$sr_pubkey" \
    --arg sr_account_id "$sr_account_id" \
    --arg sr_ss58 "$sr_ss58" \
    --argjson is_prime "$is_prime" \
    '{
      name: $name,
      mnemonic: $mnemonic,
      secret_seed: $sr_seed,
      public_key: $sr_pubkey,
      account_id: $sr_account_id,
      ss58_address: $sr_ss58,
      genesis_balance: "0",
      committees: ["Technical", "Arbitration", "Treasury", "Content"],
      is_prime: $is_prime
    }'
}

# ─── 生成创始者/Sudo 账户（Sr25519） ───

generate_creator() {
  echo ">>> 生成创始者/Sudo 账户密钥..." >&2

  local sr_output
  sr_output=$("$NODE_BIN" key generate --scheme Sr25519 2>&1)

  local mnemonic sr_seed sr_pubkey sr_account_id sr_ss58
  mnemonic=$(extract_value "$sr_output" "Secret phrase")
  sr_seed=$(extract_value "$sr_output" "Secret seed")
  sr_pubkey=$(extract_value "$sr_output" "Public key (hex)")
  sr_account_id=$(extract_value "$sr_output" "Account ID")
  sr_ss58=$(ss58_encode "$sr_pubkey")

  jq -n \
    --arg name "创始者 + Sudo" \
    --arg mnemonic "$mnemonic" \
    --arg sr_seed "$sr_seed" \
    --arg sr_pubkey "$sr_pubkey" \
    --arg sr_account_id "$sr_account_id" \
    --arg sr_ss58 "$sr_ss58" \
    '{
      name: $name,
      mnemonic: $mnemonic,
      secret_seed: $sr_seed,
      public_key: $sr_pubkey,
      account_id: $sr_account_id,
      ss58_address: $sr_ss58,
      roles: ["sudo", "creator"]
    }'
}

# ─── 常量定义 ───

# 总发行量：10,000,000,000 NEX = 10000000000 * 10^12
TOTAL_SUPPLY="10000000000000000000000"
# 每个验证者余额：100,000 NEX = 100000 * 10^12
VALIDATOR_BALANCE="100000000000000000"
# 验证者数量
VALIDATOR_COUNT=3

# ─── 生成全部账户 ───

echo "生成创始者/Sudo 账户..." >&2
CREATOR=$(generate_creator)

echo "" >&2
echo "生成 3 个验证者密钥对..." >&2
V1=$(generate_validator 1)
V2=$(generate_validator 2)
V3=$(generate_validator 3)

echo "" >&2
echo "生成 3 个委员会成员密钥..." >&2
C1=$(generate_committee_member 1 true)
C2=$(generate_committee_member 2 false)
C3=$(generate_committee_member 3 false)

echo "" >&2
echo "组装 JSON 输出..." >&2

# ─── 计算创始者余额 = 总发行量 - 验证者总余额 ───
# 确保链上总发行量精确为 100 亿 NEX

VALIDATORS_TOTAL=$(python3 -c "print(${VALIDATOR_BALANCE} * ${VALIDATOR_COUNT})")
CREATOR_BALANCE=$(python3 -c "print(${TOTAL_SUPPLY} - ${VALIDATORS_TOTAL})")

echo "总发行量:     ${TOTAL_SUPPLY} (100亿 NEX)" >&2
echo "验证者总余额: ${VALIDATORS_TOTAL} (${VALIDATOR_COUNT} × 100,000 NEX)" >&2
echo "创始者余额:   ${CREATOR_BALANCE}" >&2
echo "" >&2

# 人类可读格式
CREATOR_BALANCE_HUMAN=$(python3 -c "
total = ${TOTAL_SUPPLY}
vt = ${VALIDATORS_TOTAL}
cb = total - vt
nex = cb // 1_000_000_000_000
print(f'{nex:,} NEX')
")

# ─── 构建完整 JSON ───

jq -n \
  --arg generated_at "$(date -Iseconds)" \
  --argjson creator "$CREATOR" \
  --argjson v1 "$V1" \
  --argjson v2 "$V2" \
  --argjson v3 "$V3" \
  --argjson c1 "$C1" \
  --argjson c2 "$C2" \
  --argjson c3 "$C3" \
  --arg total_supply "$TOTAL_SUPPLY" \
  --arg creator_balance "$CREATOR_BALANCE" \
  --arg creator_balance_human "$CREATOR_BALANCE_HUMAN" \
  --arg validator_balance "$VALIDATOR_BALANCE" \
  '{
    _generated_at: $generated_at,
    _warning: "此文件包含助记词和私钥种子，请妥善保管！切勿提交到 git！",
    _doc: "根据 docs/部署.md 生成，用于主网部署。所有账户均为新生成，SS58 prefix=273（X 开头）。总发行量精确100亿NEX。",

    chain: {
      name: "Nexus",
      chain_id: "nexus",
      chain_type: "Live",
      consensus: "BABE + GRANDPA",
      block_time_seconds: 6,
      token_symbol: "NEX",
      token_decimals: 12,
      ss58_prefix: 273,
      total_supply: $total_supply,
      total_supply_human: "10,000,000,000 NEX",
      existential_deposit: "1000000000",
      existential_deposit_human: "0.001 NEX"
    },

    creator: {
      name: $creator.name,
      mnemonic: $creator.mnemonic,
      secret_seed: $creator.secret_seed,
      public_key: $creator.public_key,
      account_id: $creator.account_id,
      ss58_address: $creator.ss58_address,
      genesis_balance: $creator_balance,
      genesis_balance_human: $creator_balance_human,
      roles: $creator.roles
    },

    validators: [
      ($v1 | .genesis_balance = $validator_balance | .genesis_balance_human = "100,000 NEX"),
      ($v2 | .genesis_balance = $validator_balance | .genesis_balance_human = "100,000 NEX"),
      ($v3 | .genesis_balance = $validator_balance | .genesis_balance_human = "100,000 NEX")
    ],

    committee_members: [$c1, $c2, $c3],

    staking: {
      validator_bond: "10000000000000000",
      validator_bond_human: "10,000 NEX",
      min_nominator_bond: "100000000000000",
      min_nominator_bond_human: "100 NEX",
      min_validator_bond: "1000000000000000",
      min_validator_bond_human: "1,000 NEX",
      slash_reward_fraction_percent: 10,
      all_validators_invulnerable: true
    },

    nex_market: {
      initial_price: 10,
      price_decimals: 6,
      human_price: "0.00001 USDT/NEX",
      valuation: "100,000 USDT for 10B NEX"
    },

    genesis_config_snippet: {
      _description: "将以下值填入 runtime/src/genesis_config_presets.rs",
      creator: {
        account_id_hex: $creator.account_id,
        ss58_address: $creator.ss58_address,
        rust_code: (
          "fn creator_account() -> AccountId {\n" +
          "    parse_account_hex(\"" + ($creator.account_id | ltrimstr("0x")) + "\")\n" +
          "}"
        )
      },
      validators: (
        [$v1, $v2, $v3] | [.[] | {
          name: .name,
          account_id_hex: .sr25519.public_key,
          babe_hex: .sr25519.public_key,
          grandpa_hex: .ed25519.public_key,
          rust_code: (
            "// ── " + .name + " ──\n(\n" +
            "    parse_account_hex(\"" + (.sr25519.public_key | ltrimstr("0x")) + "\"),\n" +
            "    parse_babe_hex(\"" + (.sr25519.public_key | ltrimstr("0x")) + "\"),\n" +
            "    parse_grandpa_hex(\"" + (.ed25519.public_key | ltrimstr("0x")) + "\"),\n" +
            "),"
          )
        }]
      ),
      committee_members: (
        [$c1, $c2, $c3] | {
          rust_code: (
            "fn committee_members() -> (AccountId, AccountId, AccountId) {\n" +
            "    let member1 = parse_account_hex(\"" + (.[0].account_id | ltrimstr("0x")) + "\"); // Prime\n" +
            "    let member2 = parse_account_hex(\"" + (.[1].account_id | ltrimstr("0x")) + "\");\n" +
            "    let member3 = parse_account_hex(\"" + (.[2].account_id | ltrimstr("0x")) + "\");\n" +
            "    (member1, member2, member3)\n" +
            "}"
          ),
          members: [.[] | {
            name: .name,
            account_id_hex: .account_id,
            ss58_address: .ss58_address,
            is_prime: .is_prime
          }]
        }
      )
    }
  }' > "$OUTPUT"

# ─── 输出摘要 ───

echo "" >&2
echo "=== 生成完成 ===" >&2
echo "" >&2
echo "输出文件: $OUTPUT" >&2
echo "" >&2

# ─── 发行量校验 ───

echo "=== 发行量校验 ===" >&2
VERIFY_RESULT=$(python3 -c "
creator = int('${CREATOR_BALANCE}')
validators = int('${VALIDATOR_BALANCE}') * ${VALIDATOR_COUNT}
total = creator + validators
expected = int('${TOTAL_SUPPLY}')
print(f'  创始者余额:     {creator}')
print(f'  验证者总余额:   {validators} ({${VALIDATOR_COUNT}} × ${VALIDATOR_BALANCE})')
print(f'  链上总计:       {total}')
print(f'  期望总发行量:   {expected}')
if total == expected:
    print(f'  ✓ 校验通过：总发行量精确为 {expected // 1_000_000_000_000:,} NEX')
else:
    print(f'  ✗ 校验失败：差额 {total - expected}')
    exit(1)
")
echo "$VERIFY_RESULT" >&2
echo "" >&2

echo "=== 账户摘要 ===" >&2
echo "" >&2
echo "创始者/Sudo:" >&2
jq -r '.creator | "  \(.name): \(.ss58_address) (\(.genesis_balance_human))"' "$OUTPUT" >&2
echo "" >&2
echo "验证者:" >&2
jq -r '.validators[] | "  \(.name): \(.sr25519.ss58_address) (\(.genesis_balance_human))"' "$OUTPUT" >&2
echo "" >&2
echo "委员会成员:" >&2
jq -r '.committee_members[] | "  \(.name): \(.ss58_address)"' "$OUTPUT" >&2
echo "" >&2
echo "=== genesis_config_presets.rs 代码片段 ===" >&2
echo "" >&2
echo "--- creator_account() ---" >&2
jq -r '.genesis_config_snippet.creator.rust_code' "$OUTPUT" >&2
echo "" >&2
echo "--- mainnet_initial_authorities() ---" >&2
jq -r '.genesis_config_snippet.validators[].rust_code' "$OUTPUT" >&2
echo "" >&2
echo "--- committee_members() ---" >&2
jq -r '.genesis_config_snippet.committee_members.rust_code' "$OUTPUT" >&2
echo "" >&2
echo "WARNING: $OUTPUT 包含助记词，请勿提交到 git！" >&2

# 确保 secrets 目录被 gitignore
GITIGNORE_E2E="$SCRIPT_DIR/../.gitignore"
if [[ -f "$GITIGNORE_E2E" ]]; then
  if ! grep -q "secrets/" "$GITIGNORE_E2E" 2>/dev/null; then
    echo "mytests/secrets/" >> "$GITIGNORE_E2E"
    echo "   已自动添加 mytests/secrets/ 到 scripts/e2e/.gitignore" >&2
  fi
fi
