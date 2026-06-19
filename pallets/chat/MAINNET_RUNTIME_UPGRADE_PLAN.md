# Nexus 主网 Runtime 升级规划 / Mainnet Runtime Upgrade Plan

> **文档状态**：规划稿 v1（2026-06-19）  
> **RPC 节点**：`https://rpc.nexusmall.net`  
> **代码来源**：`https://github.com/nexusmalls/nexchat.git`（`main` 分支，runtime 发版仓库）  
> **目标**：将主网从当前链上 **spec_version 102** 升级到 **`nexchat.git` 最新 runtime（spec_version 103）**

---

## 0. TL;DR

| 项 | 链上现状（实测） | 仓库目标 |
|----|------------------|----------|
| `spec_version` | **102** | **103**（发版前必须 bump，见 §4.1） |
| `spec_name` | `nexus` | `nexus` |
| `transaction_version` | 1 | 1（无 extrinsic 编码变更时可不变） |
| 最新块高 | ~616k+（查询时 `#616006`） | — |
| **`MsgIdentity` pallet** | **不存在** | **新增 index 80** |
| `ChatSync` | 已存在 | 权重/文档更新 |
| `ChatPermission` 存储 | v0 布局（含遗留好友图谱） | v1 + 分批清理迁移 |
| 升级入口 | `Sudo` + `system.set_code` 可用 | 同左 |

**结论**：链上 102 与本地 `runtime/src/lib.rs` 的 **102 不是同一份 WASM**（本地已合入 `msg-identity` 与 permission 迁移，但未升 spec）。**不能直接覆盖同名 102**，必须 **`spec_version → 103` 后** 走标准 runtime upgrade。

---

## 1. 链上基线（从 RPC 读取）

### 1.1 读取命令

```bash
# Runtime 版本
curl -s -H 'Content-Type: application/json' \
  -d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}' \
  https://rpc.nexusmall.net | jq .

# 链名 + 最新块头
curl -s -H 'Content-Type: application/json' \
  -d '{"id":2,"jsonrpc":"2.0","method":"system_chain","params":[]}' \
  https://rpc.nexusmall.net
curl -s -H 'Content-Type: application/json' \
  -d '{"id":3,"jsonrpc":"2.0","method":"chain_getHeader","params":[]}' \
  https://rpc.nexusmall.net | jq .

# 代币 / SS58
curl -s -H 'Content-Type: application/json' \
  -d '{"id":4,"jsonrpc":"2.0","method":"system_properties","params":[]}' \
  https://rpc.nexusmall.net | jq .
```

### 1.2 2026-06-19 实测结果

```json
{
  "specName": "nexus",
  "implName": "nexus-node",
  "authoringVersion": 1,
  "specVersion": 102,
  "implVersion": 1,
  "transactionVersion": 1,
  "systemVersion": 1,
  "stateVersion": 1
}
```

- **链名**：`Nexus`
- **SS58**：273 · **代币**：NEX（12 位小数）
- **Metadata 扫描**：含 `ChatSync`、`ChatInbox`、6 个 chat pallet；**不含** `MsgIdentity` / `register_device` / `set_opk_root`
- **治理**：metadata 含 `Sudo`、`set_code`（公开 RPC 未暴露 `sudo_key` 方法，属正常）

---

## 2. 目标版本（仓库 HEAD）

本地 `runtime/src/lib.rs` 当前仍声明 `spec_version: 102`，但相对**已部署的 102** 增加/变更如下（自 commit `9532cfa` 以来）：

### 2.1 新增 pallet

| Pallet | Index | 说明 |
|--------|-------|------|
| **`MsgIdentity`** | **80** | X3DH 预密钥锚（IK/SPK/OPK Merkle 根 + `prekey_epoch` + `ChatStackCaps`） |

Extrinsics：`register_device` · `set_signed_prekey` · `set_opk_root` · `bump_prekey_epoch` · `unregister_device` · `set_stack_caps` · `force_unregister_device`（Root）

Runtime API：`MsgIdentityApi`（`device_ik` / `device_spk` / `device_opk_root` / `stack_caps` / `device_exists`）

设计：`CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md` · crate README：`msg-identity/README.md`

### 2.2 既有 chat pallet 变更

| Crate | 变更要点 |
|-------|----------|
| `pallet-chat-permission` | **存储迁移 v0→v1**（隐私设置重写 + 遗留好友图谱分批清理）；场景/能力 epoch 逻辑更新 |
| `pallet-chat-core` | 与 permission/inbox 边界对齐；权重/benchmark |
| `pallet-chat-group` | 权重/benchmark 更新 |
| `pallet-chat-inbox` | 文档 + 小幅逻辑/权重 |
| `pallet-chat-sync` | EISA 文档 + 权重 |
| `pallet-chat-common` | `deposit` / `epoch` 共享构件 |

### 2.3 Runtime 级迁移（`Migrations` tuple）

```rust
// runtime/src/lib.rs — 升级时自动执行（与 102 链上相比需确认是否已跑过）
pallet_dispute_escrow::migrations::V2RemoveLockNonces
pallet_entity_order::migration::MigrateV1ToV2
```

> **注意**：permission 的 v0→v1 在 **`pallet-chat-permission::on_runtime_upgrade`** 内触发，不在上述 tuple 中。

### 2.4 链下配套（非本升级 WASM 内，但发版需协调）

| 组件 | 依赖 |
|------|------|
| NexChat DR 1:1 | 链上 `MsgIdentity` + relay OPK 控制面 |
| NexChat 客户端 | `msgIdentity` runtime API / metadata |
| 现有 MLS-Wire 1:1 / 群聊 | **不受影响**（DR 与 MLS 并行协商，§20） |

---

## 3. 版本号策略

| 字段 | 102（链上） | 103（建议目标） | 说明 |
|------|-------------|-----------------|------|
| `spec_version` | 102 | **103** | **必须递增**；否则节点认为 WASM 未变 |
| `authoring_version` | 1 | 1 | 不变 |
| `transaction_version` | 1 | 1 | MsgIdentity extrinsic 为新增 pallet，通常不改全局 tx 版本 |
| `impl_version` | 1 | 2（可选） | 节点实现迭代时可 bump |

---

## 4. 发版前工程清单（阻塞项）

### 4.1 代码

- [ ] **`runtime/src/lib.rs`：`spec_version: 103`**（及 README 中 Spec 版本表）
- [ ] 确认 `pallet-chat-permission` 迁移在 fork 上**幂等**（已有单测，需 try-runtime 再验）
- [ ] 确认 `MsgIdentity` benchmark 权重已纳入 `runtime-benchmarks` 清单

### 4.2 测试（最低线）

```bash
# Chat 全 pallet 单测
cargo test -p pallet-chat-common -p pallet-chat-permission -p pallet-chat-core \
  -p pallet-chat-group -p pallet-chat-inbox -p pallet-msg-identity -p pallet-chat-sync

# Runtime 编译
cargo check -p nexus-runtime

# CI 同款 try-runtime（workspace）
SKIP_WASM_BUILD=1 cargo test --workspace --features try-runtime

# 可选：全量
cargo test
```

### 4.3 主网 Fork 演练（Chopsticks）

```bash
# 使用仓库内配置 fork 主网状态（db 本地缓存，勿提交 Git）
npx @acala-network/chopsticks@latest -c chopsticks-fork.yml

# 另终端：对 fork (ws://127.0.0.1:8000) 提交 set_code 并观察迁移日志
```

步骤：

1. Fork 启动后确认 `specVersion == 102`
2. 注入新 runtime WASM（见 §5.2）
3. 出块后确认 `specVersion == 103`
4. 验证 `MsgIdentity` 出现在 metadata
5. 抽样调用 `register_device`（sudo 账户或测试账户）
6. 检查 permission 迁移是否分批完成（大状态可能需多个块）

### 4.4 权重

主网前在**与生产相近硬件**重跑 chat pallet benchmarks，更新 `weights.rs` 并重新编译 WASM：

```bash
cargo build --release -p nexus-node --features runtime-benchmarks
./target/release/nexus-node benchmark pallet --chain dev \
  --pallet pallet_msg_identity --extrinsic '*' --steps 50 --repeat 20 \
  --output pallets/chat/msg-identity/src/weights.rs
# 同理：permission / core / group / inbox / sync
```

---

## 5. 主网升级步骤（建议窗口）

### 5.1 准备

1. 合并发版分支，**bump spec 103**，打 tag（如 `runtime-v103`）
2. 构建 **release runtime WASM**：

```bash
cargo build --release -p nexus-runtime
# 产物（路径随 build 配置）：
# target/release/wasm/nexus_runtime.compact.compressed.wasm
# 或 node 构建输出的 compact wasm
```

3. 记录 WASM **blake2-256 / sha256** 哈希，写入发版说明
4. 通知：验证者升级 **native node 二进制**（`impl_version` 若 bump）与 WASM 同步
5. 客户端：NexChat 开启 DR 需等链上 `MsgIdentity` 可用后再默认 `VITE_DR_ENABLED`

### 5.2 提交链上升级

**发版 WASM 构建（在 `nexchat.git` 仓库根目录）**：

```bash
git clone git@github.com:nexusmalls/nexchat.git
cd nexchat   # 即本 monorepo 根目录
git checkout main   # 需含 spec_version 103

cargo build --release -p nexus-runtime

# 压缩 WASM（set_code 用这个）
WASM=target/release/wbuild/nexus-runtime/nexus_runtime.compact.compressed.wasm
sha256sum "$WASM"
xxd -p "$WASM" | tr -d '\n' > /tmp/nexus-runtime-v103.hex
echo "WASM hex 已写入 /tmp/nexus-runtime-v103.hex"
```

**路径 A — Sudo（当前链上仍启用 Sudo）**

```text
sudo.sudo(
  system.setCode(compact_compressed_wasm_hex)
)
```

- 使用 **compressed WASM**（`*.compact.compressed.wasm`）
- 确保 sudo 账户有足够余额支付 `set_code` 权重
- 建议在低流量时段执行；提前在 Chopsticks fork 演练同一 WASM

**路径 B — 技术委员会（Root 过渡后）**

使用 `RootOrTechnicalMajority`  origin 提交同等 `system.set_code`（需委员会 2/3 提案流程，按现有 governance 脚本执行）。

### 5.3 验证者 / 节点操作

1. 升级全验证者 **nexus-node** 至发版二进制（若 `impl_version` 变更）
2. 广播 WASM 哈希，确认与链上 `set_code` 一致
3. 升级后观察：无 wasm 陷阱、无 block import stall

---

## 6. 升级后验收（Go/No-Go）

### 6.1 链上自动检查

```bash
# 1) 版本
curl ... state_getRuntimeVersion → specVersion == 103

# 2) metadata 含 MsgIdentity
# （脚本扫描 register_device / pallet_msg_identity）

# 3) 块生产正常
curl ... chain_getHeader → number 连续递增
```

### 6.2 Runtime API 冒烟

通过 Polkadot.js Apps 或自定义脚本调用 `MsgIdentityApi`：

- `device_exists(account, device_id)` → 新设备注册前为 `false`
- 注册后 `device_ik` / `stack_caps` 可读

### 6.3 Extrinsic 冒烟（测试账户）

| 调用 | 期望 |
|------|------|
| `msgIdentity.registerDevice` | 押金锁定、事件 `DeviceRegistered` |
| `msgIdentity.setStackCaps` | DR 能力位写入 |
| `msgIdentity.setSignedPrekey` | SPK + 背书存储 |
| `msgIdentity.setOpkRoot` | OPK Merkle 根更新 |

### 6.4 回归

| 场景 | 期望 |
|------|------|
| MLS 群聊 commit / welcome | 不受影响 |
| pairwise MLS-Wire 1:1 | 不受影响（DR 未协商时仍走 Wire） |
| Chat inbox 注册 / 投递 | 不受影响 |
| EISA sync anchor | 不受影响 |
| Entity / Market / 其他 pallet | 无异常 extrinsic 失败 |

### 6.5 Permission 迁移

- 抽查 `PrivacySettingsOf` 新布局（无 block/whitelist 字段）
- 遗留好友图谱 storage 键逐步清空（`LegacyMigrationPhase` → `Done`）
- 若单块权重不足，迁移**跨多块**完成属预期行为

---

## 7. 回滚策略

| 情况 | 动作 |
|------|------|
| 升级后块无法导入 | 验证者协调 **回退 node 二进制** 至旧版；链上 WASM 仅能再 `set_code` 回旧 WASM（需 sudo / 委员会） |
| MsgIdentity 逻辑 bug、MLS 正常 | 客户端关闭 DR（`VITE_DR_ENABLED=false`）；链上 pallet 保留，无数据迁移回滚 |
| Permission 迁移中断 | 迁移设计为**可续跑**；修复后再次 `set_code` 同版本或 patch 版本继续清理 |

**无法**在不 `set_code` 的情况下移除已写入的 `MsgIdentity` 链上状态；规划阶段应在 fork 上充分验证。

---

## 8. 建议时间表

| 阶段 | 时长 | 内容 |
|------|------|------|
| **T-7d** | 1 周 | spec 103 分支冻结；全量测试 + benchmark；Chopsticks 演练 |
| **T-3d** | 3 天 | 验证者通知 WASM 哈希；NexChat 发版包（DR 默认关） |
| **T-1d** | 1 天 | Fork 最终 rehearsal；值班/on-call 确认 |
| **T0** | 维护窗口 | `sudo.set_code`；监控块生产 + 迁移事件 |
| **T+1h** | 验收 | §6 检查清单 |
| **T+1d** | 观察 | 开启小流量 DR（可选）；监控 `register_device` 失败率 |

---

## 9. 角色分工

| 角色 | 职责 |
|------|------|
| Runtime 开发 | spec bump、迁移、benchmark、try-runtime |
| 节点运维 | 验证者 node 升级、WASM 哈希广播 |
| 链上管理员 | sudo / 委员会 `set_code` 签名 |
| NexChat 客户端 | DR 能力开关、prekey 发布流程 |
| Relay 运维 | OPK 控制面（与 MsgIdentity 配套，非 WASM 内） |

---

## 10. 附录

### A. 链上 vs 仓库版本对照

| 来源 | spec | MsgIdentity | ChatSync | permission 迁移 |
|------|------|-------------|----------|-----------------|
| rpc.nexusmall.net | 102 | 否 | 是 | 未跑 v0→v1 |
| 仓库 HEAD（`nexchat.git` main） | **103** | 是 | 是 | 有 |
| github origin/main | 100 | 否 | 否 | 无 |

### B. 相关文档

- `CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md`
- `pallets/chat/README.md` §上线就绪
- `pallets/chat/msg-identity/README.md`
- `pallets/chat/permission/README.md` §13 迁移
- `chopsticks-fork.yml`
- `scripts/docs/NEXUS_TEST_PLAN.md`（链下 E2E）

### C. 发版后 metadata 应出现的关键字

`MsgIdentity` · `register_device` · `set_opk_root` · `set_stack_caps` · `msgIdentity` runtime API

---

---

## 11. v103 发版记录（本地构建）

| 项 | 值 |
|----|-----|
| Git 远程 | `https://github.com/nexusmalls/nexchat.git` `main` |
| `spec_version` | **103** |
| WASM 路径 | `target/release/wbuild/nexus-runtime/nexus_runtime.compact.compressed.wasm` |
| SHA-256 | `eb9643d7566d606173c7067a38c7c15bd30099063d261a725e6bd1357491f110` |
| 大小 | ~2.4 MB |

> 主网 `set_code` 使用上表 WASM；执行前请在 Chopsticks fork 上复验同一哈希。

**下一步**：`nexchat.git` 已 bump **spec 103**；在 Chopsticks fork 上演练 §5.2，通过后由 sudo 账户执行 `set_code`。
