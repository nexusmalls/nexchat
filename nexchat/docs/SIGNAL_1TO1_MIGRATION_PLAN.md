# 1:1 改用 Signal、群聊保留 MLS — 开发规划 / 1:1 on Signal, Groups on MLS — Dev Plan

> **Status / 状态**：Proposed v0.1（待评审 / pending review）
> **Date / 日期**：2026-06-14
> **Scope / 范围**：把**私聊（`d:` 会话）**的端到端加密从 OpenMLS 迁移到 **Signal 协议（X3DH + Double Ratchet）**；**群聊（`g:` 会话）继续用 MLS**（`nexchat/mls-wasm`）。
> **关联 / Related**：`docs/RELAY_RS_FRONTEND_API.md`、`docs/RELAY_RUST_REWRITE_PLAN.md`、`docs/OFFLINE_MESSAGE_MAILBOX.md`、`docs/RELAY_PERSISTENCE.md`
> **决策前提 / Premise**：relay 保持**协议无关**——只搬运密文 + 路由控制帧；它不需要理解 Signal。这与 `RELAY_RUST_REWRITE_PLAN.md`「不引入 Signal」的口径**一致**：那条说的是 relay 本体不含 Signal 逻辑，本文新增的 Signal 全部在**客户端**与**密钥发布存储**层。

---

## 0. TL;DR

CN：私聊改用 Signal（X3DH 异步建会话 + Double Ratchet 逐消息棘轮），从协议层根除当前 MLS 1:1 的三大顽疾：①给离线对端发首条消息要等握手回执；②每会话重握手导致 epoch 漂移、旧密文解不开；③owner/member 就绪不对称。群聊维持 MLS 不变。relay 不改语义，只新增几类"协议无关"的控制/邮箱帧类型来承载 prekey 与 X3DH 首包。

EN: Move 1:1 to the Signal Protocol (X3DH async session setup + Double Ratchet per-message keys), eliminating the three structural failures of the current MLS 1:1 path. Groups stay on MLS. The relay stays protocol-agnostic — it only gains a few neutral frame types to carry prekey bundles and the X3DH initial message.

**一句话边界**：`d:` 会话 → Signal；`g:` 会话 → MLS。两套栈按会话前缀路由，互不污染。

---

## 1. 背景与动机 / Context

### 1.1 当前 1:1（MLS）实现的三个根因（已在代码核实）

| # | 现象 | 根因（文件） |
|---|------|------|
| R1 | 给离线对端发不出首条消息，卡「正在建立 1:1 OpenMLS 会话…」 | owner 的 `isReady` 要求本会话收到对端 `mls_ready`，且每会话重置（`src/mls/directHandshake.ts` `start()` / `isReady()`） |
| R2 | 跨会话旧密文解不开（`WrongGroupId` / `SecretReuseError`），沉淀陈旧帧 | 每会话重握手 → epoch 漂移；MLS 状态 200ms 防抖落盘（`src/mls/openMlsEngine.ts`） |
| R3 | 重连后重复解密 → `SecretReuseError` | relay `register_account` **非破坏性全量重推**（`relay-rs/server/src/protocol.rs` `flush_chat_mailbox`/`flush_mls_mailbox`）+ 客户端成功解密不 consume、解密前不去重（`src/state/appStore.ts` `handleInbound`） |

> ⚠️ **R3 是协议无关的传输纪律问题**：换成 Signal 也会复发（Double Ratchet 用过的 message key 同样删除，重复解密同一帧照样失败）。因此 **Track 0（见 §9）必须先做或同时做**，无论是否迁移 Signal。

### 1.2 为什么 Signal 更适合 1:1 异步

| 维度 | MLS 1:1（现状） | Signal（X3DH + Double Ratchet） |
|------|----------------|-------------------------------|
| 离线冷启动发首条 | 需握手往返 / owner 等回执 | X3DH 取对端 prekey bundle，**离线也能加密首条** |
| 会话模型 | group + epoch/commit | 逐消息棘轮，**无 epoch 漂移**，无角色不对称 |
| 乱序/迟到 | secret-tree，易 `SecretReuse` | 保存 skipped message keys，更宽容 |
| 1:1 状态量 | 群机制偏重 | 每对端一份小状态（root/chain key + skipped keys） |

### 1.3 为什么群聊保留 MLS

MLS 的树状群密钥与高效 rekey 正是为群设计；Signal 做群需 sender-key 或 pairwise 扇出，是另一套机制。群聊已基于 `chatGroup` pallet + OpenMLS 跑通，**不动**。

---

## 2. 目标 / 非目标

### 2.1 目标

- G1：私聊支持**对长期离线对端可靠发送**（含首条），对端上线后能解密。
- G2：私聊**会话可续用、跨会话稳定**，不再重握手、不再 epoch 漂移。
- G3：私聊离线邮箱积压趋近 0（成功即 consume + 解密前去重）。
- G4：群聊行为零回归（MLS 不变）。
- G5：relay 保持协议无关，wire 变更最小且向后兼容。

### 2.2 非目标

- ❌ 不改群聊加密协议（继续 MLS）。
- ❌ relay 不引入 Signal 密码学逻辑（只搬运 + 路由）。
- ❌ 不在本期废除 RFC 9474 sealed-sender 投递准入（继续用于 1:1 投递反滥用）。
- ❌ 不在本期引入跨设备多端 Signal 会话同步（单列，见 §13 未决项）。

---

## 3. 架构总览 / Architecture

```
会话路由（按 convId 前缀）
  d:<a>:<b>   → Signal 1:1 栈  → signal-wasm（X3DH + Double Ratchet）
  g:<id>      → MLS 群聊栈     → mls-wasm（OpenMLS，保持不变）

加解密统一入口（路由层）
  encryptFor(convId, plaintext)  ─┬─ d: → signalEngine.encrypt(peer, pt)
                                  └─ g: → openMlsEngine.encryptByConv(convId, pt)
  decrypt(convId, ciphertext)    ─┬─ d: → signalEngine.decrypt(peer, ct)
                                  └─ g: → openMlsEngine.decryptByConv(convId, ct)

传输层（relay，协议无关）
  - 密文帧 store-and-forward（chat 邮箱，复用现有）
  - 控制/密钥帧（prekey 发布/拉取、X3DH 首包）→ 复用现有 mailbox 路由
  - RFC 9474 sealed-sender 准入（不变）

密钥发布层
  - prekey bundle 发布：链上（chatGroup.keyPackages 的「Signal prekey」并存）或 relay 热 KV
```

**关键不变量**：relay 与现有 MLS 群聊路径**不感知**会话用的是 Signal 还是 MLS；路由完全由客户端按 `convId` 前缀决定。

---

## 4. 协议选型 / Protocol Choice

- **算法**：X3DH（Curve25519，初始密钥协商）+ Double Ratchet（对称棘轮 + DH 棘轮）+ HKDF/AEAD。与 libsignal 默认套件一致。
- **实现**：新建 **`signal-wasm`** Rust crate，基于 [`libsignal`](https://github.com/signalapp/libsignal) 的 protocol 子集，用 `wasm-bindgen` 导出，**完全复刻 `mls-wasm` 的工程形态**（`npm run mls:build` 同款管线）。
  - 理由：JS 版 `libsignal-protocol-js` 已停更；主仓与 `mls-wasm` 已有 Rust→WASM 工具链。
- **sealed sender**：保留现有 RFC 9474 盲签投递准入；Signal 的 sealed-sender 概念与之不冲突（一个是元数据保护，一个是投递反滥用）。本期沿用现有准入，不叠加 Signal sealed-sender。

---

## 5. 密钥与 Prekey 设计 / Keys & Prekeys

X3DH 需要对端预先发布一份 **prekey bundle**：

| 元素 | 来源 / 生命周期 |
|------|----------------|
| Identity Key (IK) | 由账户密钥体系派生，长期稳定（每账户一把，跨设备一致性见 §13） |
| Signed Prekey (SPK) | 中期轮换（如每周），用 IK 签名 |
| One-Time Prekeys (OPK) | 一次性池，发一条用一个；池见底要补，类似现有 KeyPackage 池 |

**发布载体（复用现有设施）**：

- **方案 A（推荐）**：复用链上 `chatGroup` 的 KeyPackage 存储位，新增一类「Signal prekey bundle」记录（或并行新 storage item）。优点：与现有 `ensureChainKeyPackagePublished` 的池维护逻辑同构，离线可达性强。
- **方案 B**：prekey bundle 走 relay 热 KV（类似 `index_put`），低延迟但依赖 relay 可用性。

> 现有 `src/mls/chainKeyPackage.ts` 的"维持最小池 + 轮换"逻辑可直接迁移成 prekey 池管理。

---

## 6. 组件与改动面 / Components & Blast Radius

### 6.1 新增

| 组件 | 说明 |
|------|------|
| `mls-wasm` 同级新 crate `signal-wasm` | X3DH + Double Ratchet，wasm-bindgen 导出 |
| `src/signal/signalEngine.ts` | 对标 `openMlsEngine.ts`：init/persist、encrypt/decrypt、prekey 生成、session store |
| `src/signal/signalSession.ts` | 取代 `directHandshake.ts` 的 1:1 协调器（但**简单得多**：无 owner/member、无 epoch、无重握手） |
| `src/signal/prekeyStore.ts` | prekey 池维护 + 发布（复用 `chainKeyPackage.ts` 逻辑） |
| `src/signal/signalStore.ts` | session/prekey 持久化（对标 `mls/mlsStore.ts`，IDB） |

### 6.2 改造

| 文件 | 改动 |
|------|------|
| `src/state/appStore.ts` | 加解密**路由层**：`d:` → signal，`g:` → mls；`handleInbound` 按前缀分发；Track 0 的 consume/去重 |
| `src/mls/directConv.ts` | 保留 convId 规范化工具；握手 owner 概念对 Signal 不再需要 |
| `src/mls/mlsDecrypt`（路由入口） | 拆成 `decryptByConv` 路由到两栈 |
| `src/relay/*` | 新增 prekey 发布/拉取、X3DH 首包的帧类型（见 §7） |

### 6.3 移除 / 降级（迁移完成后）

- `src/mls/directHandshake.ts`、`src/mls/directMlsRegistry.ts` 的 1:1 用途（群聊不依赖它们）。
- 1:1 的 `kp`/`welcome`/`commit`/`mls_ready` 控制帧（群聊仍用）。

> 群聊侧 `openMlsEngine`、`chatGroup` pallet、群 MLS 控制帧**全部保留**。

---

## 7. Wire / Relay 影响（最小且向后兼容）

relay 仍是"哑"搬运层。新增帧类型按现有 mailbox 机制路由（store-and-forward + flush）：

| 新帧（建议） | 方向 | 语义 |
|------|------|------|
| `prekey_publish` | C→S | 发布/更新自己的 prekey bundle（若走 relay KV 方案 B） |
| `prekey_fetch` / `prekey_reply` | C↔S | 取对端 prekey bundle（含取走一枚 OPK） |
| `sig_init`（X3DH 首包，`d:` 密文帧的子类型或独立 ctrl） | 经现有 chat 邮箱 | 携带 X3DH 首消息头，store-and-forward 给离线对端 |

兼容策略：

- 这些帧对 relay 是不透明对象，**沿用现有 `_ctrl` / chat 邮箱路由**，relay 代码改动可最小化（甚至仅放开新 `type` 白名单）。
- 若 prekey 走链上（方案 A），relay **零改动**。
- 与 `RELAY_RUST_REWRITE_PLAN.md` 对齐：relay 不含 Signal 逻辑，wire 仅新增中性字段。

---

## 8. 数据 / 存储 / Storage

- **客户端**：IDB 新增 `signal-sessions`（每 peer 一份 Double Ratchet 状态）、`signal-prekeys`（自己的 OPK/SPK 私钥）。
- **落盘纪律（吸取 R2 教训）**：Double Ratchet 状态在**每次成功 encrypt/decrypt 后、且在对应帧 consume 之前**完成落盘（强一致，不用纯防抖）；崩溃恢复时配合 Track 0 的"解密前持久化去重"避免重复棘轮。
- **链上（方案 A）**：prekey bundle 记录 + OPK 池（消费即减，类似 KeyPackage）。

---

## 9. 收发流程 / Flows

### Track 0（传输纪律，协议无关，**优先级最高，先做**）

> 即便不迁 Signal 也应落地；迁了 Signal 同样依赖它。

1. **成功解密即 consume**：`handleInbound` 成功分支，**本地落库后**把帧 dedupKey 批量 `consumeChatMailbox`（复用已上线的 `queueChatConsume`）。
2. **解密前持久化去重**：维护落 IDB 的"已处理 dedupKey"集合；`handleInbound` 入口先查，已处理的在解密前直接返回，杜绝重复棘轮（根除 R3 的 `SecretReuse`）。

### 发送（给可能离线的对端）

1. 本地若无与 peer 的 Signal 会话 → 取对端 prekey bundle（链上/relay）→ X3DH 建会话 → 生成 `sig_init` 首包。
2. Double Ratchet 加密消息 → 经 relay（sealed-sender 准入）投递；对端离线则进 chat 邮箱。
3. **无需等待对端在线**（解决 R1）。

### 接收

1. 上线时 relay flush chat 邮箱（含 `sig_init` 首包与后续密文）。
2. 客户端先处理 `sig_init`（建立接收会话）→ 再按序解密后续密文；乱序由 skipped message keys 兜底。
3. 每条成功解密 → 落库 → consume（Track 0）。

---

## 10. 迁移 / 兼容 / Coexistence

- **新会话**：新建的 `d:` 会话直接走 Signal。
- **存量 MLS 1:1 会话**：两种策略（择一，§13 决策点）：
  - (a) **冷迁移**：存量 1:1 历史保留只读，新消息在首次双方在线时自动重建为 Signal 会话；
  - (b) **并存过渡**：按 peer 标记会话协议版本，旧 MLS 1:1 继续可收发直至自然过渡。
- **版本协商**：在 contact/会话元数据加 `e2ee: "signal" | "mls"` 标记，避免一端发 Signal 另一端只懂 MLS。
- **群聊**：完全不受影响。

---

## 11. 阶段计划 / Phased Plan

| 阶段 | 交付 | 依赖 |
|------|------|------|
| **P0 / Track 0** | 成功即 consume + 解密前持久化去重（协议无关止血） | 无 |
| **P1** | `signal-wasm` crate：X3DH + Double Ratchet + 单测向量（与 libsignal 对齐） | — |
| **P2** | prekey 池与发布（复用 `chainKeyPackage` 逻辑；方案 A/B 二选一） | P1 |
| **P3** | `signalEngine` + `signalStore`（IDB 持久化、强一致落盘） | P1 |
| **P4** | 加解密路由层：`d:`→signal / `g:`→mls；`handleInbound` 分发 | P3 |
| **P5** | relay 新帧类型（若方案 B）/ 链上 prekey（若方案 A）联调 | P2,P4 |
| **P6** | 版本协商 + 存量 1:1 迁移策略 | P4 |
| **P7** | 灰度开关、双端跨刷新/离线回归、清理旧 1:1 MLS 路径 | P5,P6 |

> 建议：**P0 立即做**（独立收益、低风险）；P1–P5 为 Signal 主线；P6/P7 收尾与灰度。

---

## 12. 风险与回滚 / Risks & Rollback

| 风险 | 缓解 |
|------|------|
| 双栈维护成本 | 路由层清晰隔离；群聊零改动；共享 IDB/relay 基建 |
| `signal-wasm` 工程量与正确性 | 复用 `mls-wasm` 管线；对齐 libsignal 测试向量；P1 先行可独立验证 |
| 落盘一致性（重复棘轮） | P0 解密前去重 + 强一致落盘双保险 |
| 存量会话割裂 | `e2ee` 版本标记 + 并存过渡（策略 b） |
| 跨设备多端 | 本期非目标；先单设备，多端单列（§13） |
| 与 relay Rust 重写并行 | relay 保持协议无关；新帧走中性字段，互不阻塞 |

**回滚**：P7 前用灰度开关；任何阶段可回退到 MLS 1:1（保留代码直至 P7 验证通过）。

---

## 13. 未决问题 / 决策点 / Open Questions

1. **prekey 发布载体**：链上（方案 A，离线可达强、上链成本）vs relay KV（方案 B，低延迟、依赖 relay）。
2. **存量 1:1 迁移**：冷迁移(a) vs 并存过渡(b)。
3. **跨设备多端**：Signal 单会话天然单设备；多端需 sesame/设备链，本期是否纳入。
4. **Identity Key 来源**：复用账户签名密钥派生，还是独立 E2EE 身份密钥（涉及备份/恢复）。
5. **sealed sender**：是否在后续叠加 Signal 原生 sealed-sender 增强元数据保护。

---

## 14. 验收 / 测试 / Acceptance

- 单测：`signal-wasm` 与 libsignal 测试向量逐字节一致；Double Ratchet 乱序/跳号/重放用例。
- 集成：两端跨多次刷新 + 一端长期离线，验证 G1/G2/G3（首条可达、会话续用、积压归零）。
- 回归：群聊 `g:` 全流程零改动（G4）。
- 端到端：复用 `scripts/` 现有 flow（参考 `scripts/docs/NEXUS_TEST_PLAN.md`），新增 1:1 离线场景。
