# Chat 多设备 MLS 会话同步 · 设计方案

> 状态：设计草案 · **v4 修订**（**群轨 A 已废止**；现行 = **群 Wire 化 + 1:1 Wire 多 leaf**；终态 = **路线 B 虚拟客户端**）
> 适用范围：同一账户多设备「同步聊天」——换机、平板/桌面副设备、群/1:1 并发收发
>
> **v4 修订要点**：
> ① **废止**「群=轨 A 托管 ⊕ 1:1=Wire」混合方案（原 `CHAT_MULTIDEVICE_HYBRID_DESIGN.md`，已删除）；
> ② **群多设备现行设计 of record** 迁至 [`CHAT_GROUP_WIREIFY_DESIGN.md`](./CHAT_GROUP_WIREIFY_DESIGN.md)（每设备独立 leaf，无主/附/PIN/交接）；
> ③ 本文 **§4 保留轨 A 群侧历史与已落地代码索引**（供退役对照），**不再作为产品方向**；
> ④ **§6 路线 B** 仍为长期终态（Gate-B 阻断）；§3.1 密码学约束对 A/B/Wire 均成立。
>
> 关联（仓内）：
> - [`CHAT_GROUP_WIREIFY_DESIGN.md`](./CHAT_GROUP_WIREIFY_DESIGN.md)（**群多设备现行落地文档**）
> - [`CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md`](./CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md)（1:1 Wire：CD 选举 / relay CAS / 设备加入 / E2EI）
> - `pallets/chat/sync/`（EISA 同步锚：通讯录 / 聊天记录 / 会话索引）
> - `pallets/chat/inbox/`（relay 盲签投递锚点：inbox epoch + 定向标签撤销）
> - `pallets/chat/group/src/lib.rs`（1:1 不建链上群不变量；`commit(expected_epoch)` DS/AS）
>
> 关联（标准/外部）：RFC 9750 §6.7 / §8.2.4、RFC 9420 §3.7；
> **活跃草案** `draft-ietf-mls-virtual-clients`（**未定稿**，路线 B 依赖，见 §10 Gate-B）。

---

## 0. 一句话结论 / TL;DR

CN：**现行（Gate-B 转绿前）** = **群与 1:1 同构的 Wire 多 leaf**：每台登录设备持**本设备 leaf 私钥**（不托管），
**直接并发收发**；新设备须被 **Add** 进各活跃群/会话（后台自动化、用户无感），历史正文仍由 **`K_archive` 助记词自愈**。
**终态** = **路线 B 子群虚拟客户端**（对外单一 leaf、隐藏设备数、`reuse_guard` 并发、保 PCS）——受 **Gate-B** 阻断。

**废止**：群 **轨 A**（共享单 leaf + 签名钥剔除 + primary/secondary + 在线交接 + PIN）——根因是「只读设备不能发」
产品陷阱与 PCS 让渡；已落地代码见 §4，**按 WIREIFY 计划退役**。

EN: **Current (pre–Gate-B)** = **Wire multi-leaf for both groups and 1:1**: each logged-in device holds its **own leaf
private key** (never escrowed), **sends concurrently**; new devices must be **Added** to active groups/sessions (automated
in background). **Endgame** = **Track B subgroup virtual client** (single outward leaf, hidden device count, full PCS) —
**blocked by Gate-B**.

---

## 1. 目标与非目标

### 1.1 能力矩阵（现行 Wire vs 终态路线 B）

| 能力 | 现行 · 群/1:1 Wire 多 leaf | **终态 · 路线 B 虚拟客户端** |
|---|---|---|
| 多端并发发送（群 + 1:1） | ✅ 每设备独立 ratchet | ✅ `reuse_guard` PRP |
| 换机后读历史正文 | ✅ `K_archive`（与 MLS 无关） | ✅ 同左 |
| 换机后发/解实时消息 | ⚠️ 须被 Add（§5.3） | ✅ 加入设备子群即派生全部会话钥 |
| 无主/附/PIN/在线交接 | ✅ 无（Wire 目标） | ✅ 无 |
| 群内设备数隐私 | ❌ 成员可见 N leaf | ✅ 对外单一 leaf |
| PCS（丢设备自愈） | ✅ Remove leaf | ✅ 移除子群成员 |
| 助记词泄漏 → 读未来消息 | ✅ 不可（leaf 不托管） | ✅ 不可 |
| 链改 | 近零（群：`empty-delta` commit，见 WIREIFY §4） | 在线近零；离线自助可选 External Commit |
| 生产就绪 | ⚠️ 群 Wire 待 G0 spike；1:1 Wire 已落地 | ❌ Gate-B 红 |

### 1.2 非目标（A/B/Wire 共同）

- **历史 epoch 协议级恢复**：历史**可读正文**来自 archive，不来自 MLS 状态。
- **spent 防重放集合迁移**：沿用 EISA 同步锚（`pallets/chat/sync/`）的密文 manifest 迁移约定。
- **全员离线超 relay 保留期仍不丢消息**：物理边界，任何 E2EE 方案均存在。

---

## 2. 三个数据面（不变）

| 数据面 | 内容 | 密钥 | 多设备方案影响 |
|---|---|---|---|
| **应用数据** | 通讯录 / 聊天记录 / 会话索引 | `K_contacts` / `K_archive` / `K_index` | ❌ 正交；助记词完全自愈 |
| **群 MLS 会话态** | OpenMLS 群状态 | **Wire：每设备 leaf 私钥本机自持**（不进 vault） | ✅ 群 Wire 化 |
| **1:1 MLS 会话态** | pairwise 多 leaf | 每设备自持（已落地） | ✅ 1:1 Wire |

> **轨 A 的 `K_mls_escrow` vault 托管群状态**（§4）与 Wire 模型**互斥**——Wire 化后群侧 vault 只读路径退役。

---

## 3. 核心密码学约束（必读）

### 3.1 硬约束：为何不能「共享一套状态、所有设备并发发」

MLS 每个 leaf 一条发送 ratchet。两台设备**共享同一条 ratchet、各自独立发消息** → 相同 `(key, nonce)` →
**AEAD nonce 重用** → 机密性/完整性崩塌。并发 Commit 亦会分叉。

**推论**：E2EE 下多端并发发，只有三条路：

1. **每设备独立 leaf**（= 现行 Wire 化）——已实现于 1:1，群见 WIREIFY；
2. **`reuse_guard` PRP 切分 nonce 空间**（= 路线 B 虚拟客户端）——Gate-B 阻断；
3. **放弃 E2EE**（服务端持钥）——本产品不走。

> 「完全不要 leaf、又共享、又并发、又 E2EE」四者不可同时成立。产品可做的是 **Wire + 自动化 Add + 隐藏 UI**，
> 不是消灭底层 per-device 发送状态。

### 3.2 定序后端差异（群 vs 1:1 Wire）

| 会话 | 定序 | 说明 |
|---|---|---|
| **1:1** `d:` | relay `commit_slot` CAS（闸二） | 链下 pairwise，无链上 epoch |
| **群** `g:` | 链上 `commit(expected_epoch)` | 区块全序；**不需要** relay commit_slot |

---

## 4. 【已废止】轨 A · 群加密状态托管

> ⚠️ **本节仅作历史与已落地代码索引。产品方向已废止**，详见 WIREIFY。新功能**不得**再依赖轨 A 群侧
> primary/handoff/PIN/`groupSendMode`。

轨 A 思路：单共享 leaf，`exportEscrowState()` 剔签名钥 → `K_mls_escrow` vault → 单活跃发送 + §5 交接。

| 子系统 | 状态 | 退役动作（WIREIFY G6） |
|---|---|---|
| `mlsVaultSync` / `K_mls_escrow` 群路径 | 已落地，默认休眠 | 群引擎改持本设备 signer 后停用群 vault 导出 |
| `groupHandoffRuntime` / `sendingAuthority` / PIN 备份 | 已落地 | 删除群侧依赖与 UI |
| `exportEscrowState` / `importEscrowVault` 群只读恢复 | 已落地 | 群侧停用；1:1 不受影响 |
| PCS 让渡披露（§6 原稿） | P0 已拍板 opt-in | Wire 化后群侧不再需要 |

**保留价值**：EISA `SyncManifest.mls` 字段与 vault 合并算法仍可服务**非 MLS 用途**或过渡期只读引导，但**不是**群发送权模型。

原 §3.2–§7、§9.1 全文不再维护；需要细节请查 git 历史 v3 稿或已落地代码注释。

---

## 5. 现行方向 · 群 Wire 化 + 1:1 Wire 多 leaf

**设计 of record**：[`CHAT_GROUP_WIREIFY_DESIGN.md`](./CHAT_GROUP_WIREIFY_DESIGN.md)

### 5.1 模型摘要

- 群引擎与 1:1 各用独立 `OpenMlsEngine`（`signer: Option` 为 client 级）。
- leaf 身份：`deviceLeafIdentity = {account}#{deviceId}` + E2EI 设备 leaf 凭证（`deviceLeafCredential.ts`）。
- **leaf 私钥绝不**由 `vault_master` 派生、**绝不**进 vault。
- 链上 `GroupMembers` 仍按**账户**一条；设备 leaf 是 MLS 树内部细节。

### 5.2 复用映射（1:1 → 群）

| 1:1 已落地 | 群 Wire 化 |
|---|---|
| `directWireSession` / `directAccountCommitCoordinator` | CD 选举 + `device_join_*` |
| `directWireCommitExecutor`（staged commit） | `createGroupDeviceExecutor` + **上链** `commit` |
| `followCommitGuard` / E2EI | 成员侧复验设备 add commit |
| relay `commit_slot` CAS | **不用**；改用链上 `expected_epoch` |
| `wireDeviceRoster` / `WireDeviceSheet` | 群侧同款披露 |

### 5.3 新设备须被 Add（用户无感自动化）

新设备不在 MLS 树里 → 不能解实时 E2EE 消息。**Add** = 已在群内的 leaf 发 Commit 把新设备 leaf 插入树 + Welcome。

| 情形 | 谁 Add | 体验 |
|---|---|---|
| 我有兄弟设备在线 | CD 自动 `add_device` | 登录后数秒可发 |
| 我全离线、群成员在线 | peer-add-device（E2EI 校验） | 稍等 |
| 全离线 | 等上线或 External Commit | 可读 archive 历史，发需等待 |

流程细节、empty-`member_delta` 链上 commit、延迟 Add 缓解 fan-out：**见 WIREIFY §4–§8**。

### 5.4 群成员准入（与 Wire 正交，链上不变）

| 群类型 | 新**账户**入群 | 新**设备**（已是成员） |
|---|---|---|
| **私有群** | `request_join` → `approve_join` → Add | **不需**群主审批；账户内 CD + E2EI |
| **公开群** | opt-in（KeyPackage）+ 管理员 Add | 同左 |

人数上限：公私群共用 `MaxGroupMembers = 500`（计**账户**；设备 leaf 不计入）。

---

## 6. 终态 · 路线 B（子群虚拟客户端）

> 完整设计保留 v3 §8.3–§8.9 语义；此处为 v4 摘要。**受 Gate-B 阻断，不得直接进生产**（§10）。

**模型**：用户所有设备组成**链下设备子群** → 从子群 epoch **确定性派生**各「超群」（真实群 + 1:1）的**单一虚拟 leaf**。
对外只见一个成员；并发靠 `reuse_guard` PRP（§3.1 修正）。

**相对 Wire 的收益**：

- 加/删设备 = **设备子群内 1 次 Commit**（O(1)），非 O(活跃群数)；
- **隐藏**群内设备数；
- 1:1 与群**完全同构**，换机「加入子群一次 = 拿到全部会话」。

**相对 Wire 的代价**：依赖未定稿标准 + OpenMLS experimental feature；设备子群审批 UX 为安全必需。

### 6.1 不丢消息（§8.6 摘要）

1. relay **回显**同账户兄弟设备（`echoSelf`，已落地）；
2. 设备子群状态传播（补 1:1 无 HandshakeLog）；
3. 留存到 ack（至少一台设备 ack 才淘汰）。

### 6.2 迁移 Wire → 路线 B（Gate-B 转绿后）

群 + 1:1 从「多 leaf」收敛为「虚拟客户端」；各超群重 rekey 一次；archive/contacts **无需迁移**。

---

## 7. 落地清单

### 7.1 群 Wire 化（现行，详见 WIREIFY §11）

| Phase | 交付 | Gate |
|---|---|---|
| **G0** | Spike：empty-`member_delta` commit + 多 leaf 并发/PCS | **Gate-G1 Blocking** |
| **G1–G3** | 群引擎 Wire + 加删设备 + CD + 链上定序 | 依赖 G0 |
| **G4–G5** | 延迟 Add / peer-add 兜底 | |
| **G6** | 退役轨 A 群侧（§4 表） | |
| **G7** | E2E | |

### 7.2 路线 B（终态，Gate-B 转绿后）

| Phase | 交付 |
|---|---|
| **Gate-B** | 标准定稿 + OpenMLS virtual-clients 稳定 + B-spike |
| **B-P1–B-P4** | 设备子群 + 虚拟客户端派生 + 1:1/群统一 + E2E |

---

## 8. 方案选型

| 方案 | 结论 |
|---|---|
| **群/1:1 Wire 多 leaf** | **现行（Gate-B 前）** |
| **路线 B · 虚拟客户端** | **终态首选**（Gate-B 阻断） |
| **轨 A · 群状态托管** | **废止**（§4） |
| 共享 leaf 并发发 | **否决**（§3.1 nonce 重用） |
| 服务端持群钥（非 E2EE） | **否决**（产品定位） |

---

## 9. 测试与验收（摘要）

- **Wire 群**：G0 spike S1–S5（WIREIFY §11.1）；多端并发群发；换机 archive + Add 续发；Remove PCS。
- **Wire 1:1**：沿用 `directWireSession*.test.ts` 与 SERIALIZATION_SPEC 单测。
- **路线 B**：Gate-B 后 §6 + 原 v3 §10.B 向量。
- **E2E**：`scripts/e2e/` 既有 runner，新增「群多端并发 + 换机」场景。

---

## 10. 附录 · Gate-B 与 relay 前置（摘要）

### 10.1 Gate-B 判定（2026-06）：**红**

- OpenMLS **0.8.1** 无 `virtual-clients` feature；草案 `draft-ietf-mls-virtual-clients` 仍 WIP。
- B-spike 须在独立 crate 用上游 git rev，**不进生产 mls-wasm**，直至 release 稳定。

### 10.2 链上 Phase 0（已验证）

- 同账户 **empty-`member_delta` commit**（rekey / 加设备 leaf 的链上投影）**已放行**——`same_account_empty_delta_commit_rekey_is_allowed`（`group/src/tests.rs`）。
- 真·跨账户非成员自助入群仍 `NotMember`——仅 External Commit 通道可解（可选）。

### 10.3 relay 已落地（与 Gate-B 解耦，休眠）

- `echoSelf` 兄弟设备回显；
- `s:<account>` 账户内扇出 + `device_join_*` 控制面；
- `msgId` 去重；1:1 闸二 `commit_slot` CAS（`relay-rs/core/src/commit_slot.rs`）。

---

## 11. 参考

- RFC 9750 / RFC 9420；`draft-ietf-mls-virtual-clients`（跟踪中，勿当稳定规范引用）。
- 生产参考：`wireapp/core-crypto`（多 leaf，非子群）；OpenMLS 0.8.1（本仓锁定）。

---

## 文档索引（pallets/chat/）

| 文档 | 角色 |
|---|---|
| **本文档** | 多设备总纲：密码学约束 + 废止轨 A + Wire 现行 + 路线 B 终态 |
| [`CHAT_GROUP_WIREIFY_DESIGN.md`](./CHAT_GROUP_WIREIFY_DESIGN.md) | **群 Wire 化落地 of record** |
| [`CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md`](./CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md) | 1:1 Wire 串行化 / 加入 / E2EI |
| [`CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md`](./CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md) | 1:1 X3DH + Double Ratchet（与 MLS 解耦的替代栈） |
| `pallets/chat/sync/` | EISA 云同步锚（通讯录 / 聊天记录 / 会话索引 IPFS 自愈） |
| `pallets/chat/inbox/` | 链下盲签投递锚点（inbox epoch + 定向标签撤销） |

已删除（v4 及后续收口）：`CHAT_MULTIDEVICE_HYBRID_DESIGN.md`、`CHAT_MODULES_CONSOLIDATION_DESIGN.md`、
`CHAT_SYNC_ANCHOR_ADR.md`、`CHAT_OFFCHAIN_DELIVERY_DESIGN.md`、`CHAT_GROUP_CLIENT_INTEGRATION.md`、
`CHAT_P2_SESSION_ANCHOR_DESIGN.md`、`CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md`、`CHAT_DEVICE_RETENTION_DESIGN.md`
（结论已落入对应 pallet 模块头与 README；链下细节属链下组件）。
