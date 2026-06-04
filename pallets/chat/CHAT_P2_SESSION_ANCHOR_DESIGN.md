# Chat P2 · 私聊会话锚点与跨设备发现 · 设计与决策

> 状态：决策已定（**链上否决 + 链下方案**），客户端 + 可选 relay 职责，链上不参与。
> 适用范围：链下人类私聊在「消息」页的会话发现 / 跨设备恢复。
> 关联：
> - `pallets/chat/group/src/lib.rs`「1:1 不建链上群」隐私不变量
> - `pallets/chat/core/src/lib.rs`（`Session` / `create_session`，仅 `System` 上链）
> - `CHAT_OFFCHAIN_DELIVERY_DESIGN.md`（盲化投递令牌 / inbox）
> - `CHAT_DEVICE_RETENTION_DESIGN.md`（设备端保留：会话索引/恢复）
> - `pallets/chat/README.md`「链上 / 链下边界」+「客户端 Merge Spec」

## 0. 一句话结论 / TL;DR

CN: 曾设想给链下私聊加一个链上 `touch_session` / `open_direct_session` 锚点，让会话在
`chat_listConversations` 中出现。**否决**——它会把"谁和谁聊"重新写回链上明文，违反本系统
核心隐私不变量。真正要解决的是"换设备/重装后会话列表从哪恢复"，这是**链下职责**：以
**加密会话索引 blob**（方案 A，首选）为主、**inbox 投递推导**（方案 B）兜底。链上**不新增**
任何 extrinsic / storage / event。

EN: A proposed on-chain `touch_session` anchor for off-chain DMs is REJECTED: it would
re-publish the communication relationship (who↔whom) in clear and break the core privacy
invariant. The real need — recovering the conversation list on a new device — is an
OFF-CHAIN concern, solved by an encrypted conversation-index blob (Option A, preferred)
with inbox-derived discovery (Option B) as fallback. No on-chain extrinsic/storage/event
is added.

---

## 1. 被否决的方案 / Rejected: on-chain session anchor

### 1.1 设想

新增 extrinsic（如 `touch_session(peer)` / `open_direct_session(peer)`），只写会话元数据、
不写消息内容，使链下私聊在 `chat_listConversations` 的私聊行中"有锚点"。

### 1.2 否决理由

**（1）违反隐私不变量。** `pallet-chat-group` 明确承诺「1:1 私聊不建链上群」，因为建群会公开
成员关系。链上私聊会话锚点是同一类泄漏的另一种写法。核对 `core::create_session` 实际写入：

| 写入项 | 内容 | 泄漏 |
|--------|------|------|
| `Session.participants` | `[alice, bob]` 明文 | 直接暴露通信关系 |
| `session_id = hash(sorted[alice,bob])` | 成对哈希 | 第三方可对候选账户对**碰撞验证**是否聊过 |
| `UserSessions(who, sid)` | 每用户会话索引 | 暴露账户的会话数量 + 对端集合 |
| `Event::SessionCreated{participants}` | 明文事件 | 历史永久可追溯 |

**（2）技术上无"隐私保护的链上锚点"。** 链是公开的：按 `who` 可枚举的索引必然暴露数量与
对端；`hash(pair)` 可被碰撞反查；即便只存承诺值，按账户的存在性/时序也会泄漏。隐藏多人/成对
关系需要匿名凭证等另一类原语，超出 MLS DS+AS 锚的范围。

**（3）没有真实必要。** 聊天客户端为解密 MLS 本就持有本地会话状态——会话列表在客户端已存在。
唯一站得住的需求是**跨设备/重装后的会话发现**，而那不需要链上锚点（见 §2）。

> 既有事实（非本次引入）：`System` 通知（订单/仲裁）确实经 `create_session` 在链上留下
> buyer↔seller 明文会话。这是 System 通道的固有取舍（业务关系本在 order/dispute pallet 公开），
> **不得**据此把人类私聊也锚上链。

### 1.3 链上结论

**不新增** extrinsic / storage / event；`core` 维持现状（仅 `System` 经 `create_session` 上链）。

---

## 2. 链下方案：跨设备会话发现 / Off-chain cross-device discovery

把"会话出现在列表"重述为产品需求：**用户在新设备登录后如何恢复会话列表（含偏好）**。

### 2.1 方案 A：加密会话索引 Blob（首选）

- 客户端维护一份**加密会话索引**（conversation index），内容：
  - 每个会话：对端身份引用、MLS group ref、会话类型、置顶 / 免打扰偏好、最后已读位点、
    最后活跃时间（客户端本地真值，用于跨类型排序）。
- 用账户主密钥派生的对称密钥加密后存 IPFS：`K_index = KDF(account_master, "chat/conv-index/v1")`。
- **指针不上链**：CID 经该账户的 inbox 队列（relay 侧，键为不可关联的 `inbox_id`）下发，或由
  固定派生路径让新设备自取；链上零新增。
- 新设备流程：解锁主密钥 → 取并解密 index blob → 还原会话列表与偏好 → 再用 MLS 状态 +
  inbox 补投恢复消息正文。
- 隐私：链上零新增；relay 仅见密文 + `inbox_id`（不可关联账户）。
- 代价：blob 的**多设备并发写**需合并策略（见 §3）。

#### Blob schema（建议，客户端实现，仅约定不强制）

```jsonc
{
  "v": 1,
  "updated_at": 1733300000,           // 单调时钟，用于 LWW/合并
  "device_id": "…",                   // 最后写入设备
  "conversations": [
    {
      "kind": "direct",               // "direct" | "group"
      "peer_ref": "…",                // 对端身份引用（不落链）
      "mls_ref": "…",                 // 私聊成对 MLS 会话引用
      "pinned": false,
      "muted": false,                 // 本地免打扰偏好（≠ 群管理员禁言）
      "last_read": "…",               // 已读位点（MLS generation / 本地序号）
      "last_active": 1733299900        // 客户端真值，用于排序
    },
    {
      "kind": "group",
      "group_id": 42,                 // 链上群 id（群成员本就公开）
      "pinned": true,
      "muted": false,                 // 本地免打扰偏好（与链上禁言区分）
      "last_read": "…",
      "last_active": 1733299950
    }
  ]
}
```

### 2.2 方案 B：inbox 投递推导（兜底）

- 不维护显式索引；新设备枚举自己 inbox 的历史投递记录（relay 持有，按 `inbox_id` 键）+
  MLS Welcome 历史，**推导**会话集合。
- 隐私：链上零新增。
- 限制：**偏好**（置顶 / 免打扰 / 已读位点）无法从投递记录恢复——需另存，退化为方案 A 的子集。
  故方案 B 仅作冷启动兜底（"先把会话拉出来"），偏好仍依赖方案 A。
- 依赖：relay 须保留足够历史，与 `CHAT_OFFCHAIN_DELIVERY_DESIGN.md` 的保留策略对齐。

### 2.3 与统一会话视图的关系

- `chat_listConversations` 仍只返回**链上切片**（System 私聊会话 + 群元数据）。
- 客户端按 `pallets/chat/README.md` 的 **Merge Spec** 合并：
  - 会话集合 = 方案 A index（首选）∪ 方案 B 推导（兜底）∪ 链上切片；
  - 排序 = `max(链下 last_active, 链上 last_active)`，置顶优先；
  - 同对端的 System 会话与人类私聊按对端合并为一张卡片。

---

## 3. 多设备并发写合并 / Multi-device merge

方案 A 的 index blob 可能被多设备并发更新，必须定合并策略，否则丢更新：

- **基线（推荐起步）**：字段级 Last-Writer-Wins，按每会话每字段的 `updated_at` 取较新；删除用
  墓碑（tombstone）保留一段时间防"复活"。
- **升级路径**：若并发频繁，迁移到 per-field CRDT（如 LWW-Map / OR-Set）。
- 写入并发控制：取-改-存时带版本号，relay/IPFS 侧用乐观锁；冲突则拉取最新再合并重试。

---

## 4. 落地清单（本决策的产出）

| 项 | 链上改动 | 文档落点 |
|----|----------|----------|
| 否决链上锚点 | **无** | 本文 §1 + `pallets/chat/README.md` 交叉引用 |
| 方案 A / B（跨设备发现） | **无** | 本文 §2–§3；`CHAT_DEVICE_RETENTION_DESIGN.md` 的"会话索引"小节与之衔接 |

客户端方案的实现与测试归客户端仓；本仓库 pallet 不新增任何接口。

---

## 5. 明确不做 / Non-goals

- 不为私聊增加任何链上会话 / 锚点 / 索引 / 事件。
- 不在链上维护会话偏好（置顶 / 免打扰 / 已读位点）——除既有 System 会话外，均链下。
- 不试图在链上表达人类消息的未读 / 活跃度。
- 不引入"隐私保护的链上社交图"（需匿名凭证等另立项目）。

---

## 6. 待评审点 / Open questions

1. index blob 指针的下发通道：复用 inbox 队列 vs 固定派生路径自取——择一定稿。
2. relay 历史保留窗口是否足以支撑方案 B 兜底（与投递规范对齐）。
3. 产品：同对端的"订单 System 通知"与"人类私聊"是否同卡片混排，还是分区展示。
