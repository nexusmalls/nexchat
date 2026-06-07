# Chat Group 客户端集成时序规范 / Group Client Integration Spec

> 状态：实现对齐（链上 DS/AS 已落地，本文规约客户端调用顺序）
> 适用范围：`pallets/chat/group`（MLS DS/AS 锚）+ 其只读 `ChatGroupApi` / `chat_*` RPC + OpenMLS 客户端
> 关联：`group/README.md`、`CHAT_OFFCHAIN_DELIVERY_DESIGN.md`（投递）、`CHAT_LARGE_FILE_SPEC.md`、
> `common/src/runtime_api.rs`（统一会话视图）

## 0. 一句话结论 / TL;DR

CN: 链只做 MLS 的 **Delivery Service + Authentication Service**：为身份（KeyPackage）与成员变更
（Commit / epoch）全局定序，存 Welcome 短期信箱与群元数据。**所有密码学与消息密文在客户端
（OpenMLS）+ relay**。本文规约客户端为「不与链状态打架」必须遵守的调用顺序，重点是
**`pending_welcome`（只读取回）→ 本地处理 → `claim_welcome`（删信箱）** 的先读后删次序，以及
P0 后新增的三条链上不变量（welcome/delta 双射、禁止 2 人群、Add 须有 KeyPackage）。

EN: The chain is only the MLS DS+AS. All crypto and ciphertext live in the OpenMLS client + relay.
This doc fixes the client call ordering that keeps local MLS state consistent with the chain — chiefly
**read `pending_welcome` → process locally → `claim_welcome` (deletes the mailbox)** — plus the three
on-chain invariants added in P0.

---

## 1. 链上 / 链下职责回顾 / Responsibility recap

| 链上（本 pallet） | 客户端 / relay（仓库外） |
|---|---|
| KeyPackage 发布/吊销、计数 | TreeKEM / HPKE / AEAD 加解密（OpenMLS） |
| `commit` 定序、epoch 单调、防分叉 | 构造 Commit / Welcome / GroupInfo 字节 |
| Welcome 短期信箱、HandshakeLog | 群消息密文投递（relay，§见投递规范） |
| 成员表 / 角色 / 封禁 / 禁言状态 | 禁言与消息的实际拦截 |
| 群资料（名/头像/公告） | 未读、排序、活跃度、置顶/免打扰 |

> 不变量：**1:1 私聊不建链上群**；链上群成员数恒为 1（仅创建者）或 ≥3。

---

## 2. 身份准备 / KeyPackage lifecycle

```text
client: 用 OpenMLS 生成 KeyPackage(kp_bytes)
  → tx publish_key_package(kp_bytes)        # 预留 KeyPackageDeposit；计数 +1
  → 事件 KeyPackagePublished{ who, id }
```

- **被加入任何群（公开或私有）的前置条件**：`KeyPackageCount(who) > 0`（链上强制，P0）。
- 轮换 / 怀疑泄露：`revoke_key_package(id)`（退押金）。吊销后**不再可被加入新群**，但**已在群内
  的成员身份不受影响**。
- 客户端应维护一个「在册 KeyPackage 池」并按需补发，避免被加时无可用预共享公钥。

---

## 3. 建群 / Create group

```text
client: OpenMLS 本地初始化 group（epoch 0，仅创建者），算出 tree_hash / transcript_hash
  → tx create_group(init_group_info_cid, cipher_suite, is_public, tree_hash, transcript_hash)
  → 事件 GroupCreated{ group_id, creator, epoch:0 }；creator = Owner；预留 GroupDeposit
```

约束：建群冷却 `GroupCreationCooldown`、单账户群上限 `MaxGroupsPerUser`。

> ⚠️ **首次扩群必须一次加入 ≥2 人**：群从 1 人直接到 2 人会被 `TwoMemberGroupForbidden` 拒绝。
> 建群后第一条成员 `commit` 的 `added` 至少 2 个账户（1 → 3）。

---

## 4. 加人 / 成员变更（commit）/ Add & membership change

### 4.1 公开群

```text
（被加者此前已 publish_key_package）
client(owner/admin): OpenMLS 生成 Commit + 每个新成员的 Welcome + 新 GroupInfo
  → tx commit(group_id, expected_epoch, commit_bytes,
               new_tree_hash, new_transcript_hash, new_group_info_cid,
               welcomes = [(addee_i, welcome_i), ...],
               member_delta = { added:[addee_i,...], removed:[...] })
```

### 4.2 私群

```text
applicant: tx request_join(group_id)               # 非成员、未被封禁
owner/admin: tx approve_join(group_id, applicant)   # 写 JoinApprovals
owner/admin: tx commit(... 同上 ...)                # commit 消费 approval
```

### 4.3 链上对 `commit` 的三条强校验（P0）— 客户端必须满足

1. **welcome / delta 双射**：`welcomes` 与 `member_delta.added` **一一对应**——长度相同、每个新成员
   **恰一条非空** Welcome、无多余条目。否则 `WelcomeMismatch`。
2. **KeyPackage 闸门**：每个 `added` 账户 `KeyPackageCount > 0`，否则 `AddeeNotJoinable`。
3. **禁止 2 人群**：本次变更后 `member_count != 2`，否则 `TwoMemberGroupForbidden`。
   - 推论：3 人群移除 1 人会被拒——要么一次移除 2 人到 1 人（仅剩群主），要么先补人。

权限：加他人 / 删他人需 Owner 或 Admin；普通成员仅可「自助退群」（`removed=[self]` 且自己非群主）。

---

## 5. ★ 新成员入群：先读后删 / Welcome retrieval ordering

**这是本规范的核心**。Welcome 在 `claim_welcome` 时被**删除**且**不回传字节**，故必须先读后删：

```text
addee（被 commit 加入后）:
  1. RPC  chat_pendingWelcome(group_id, who)  →  welcome_bytes | null   # 只读，不消费
     （或 state_call ChatGroupApi_pending_welcome）
  2. 本地 OpenMLS 处理 welcome_bytes，加入群、建立本地会话
  3. RPC  chat_handshakeAtEpoch(group_id, e)  对每个缺失 epoch 补齐 Commit（见 §6）
  4. tx   claim_welcome(group_id)             # 确认入群、删除信箱，发 WelcomeClaimed
```

❌ 反模式：先发 `claim_welcome` 再读 → 信箱已删，`pending_welcome` 返回 `null`，Welcome 永久丢失。
✅ 仅当第 2 步本地成功消费 Welcome 后，才提交第 4 步的 `claim_welcome`。

> `claim_welcome` 是「我已收到并处理 Welcome」的链上确认 + 信箱回收，**不是**获取 Welcome 的途径。
> 即使客户端崩溃，只要未 claim，重启后仍可用 `pending_welcome` 再次取回。

---

## 6. epoch 补齐与并发仲裁 / Catch-up & concurrent commits

- **离线补齐**：`HandshakeLog[group_id][epoch]` 每个 epoch 一条合并后的 Commit 字节。落后的成员
  从本地已知 epoch+1 起，逐 epoch `chat_handshakeAtEpoch` 拉取并喂给 OpenMLS，直至追上
  `group_mls_snapshot.epoch`。
- **并发 commit 仲裁**：`commit` 带 `expected_epoch`，链按区块全序只接受等于当前 epoch 者；落败方
  得 `EpochStale`。客户端策略：
  ```text
  收到 EpochStale → 重新 chat_groupMlsSnapshot 取最新 epoch
                 → 用最新 HandshakeLog 重建本地状态
                 → 重算 Commit/Welcome，以新 expected_epoch 重试
  ```

---

## 7. 群主转让 / Ownership transfer

```text
owner: tx transfer_ownership(group_id, new_owner)   # new_owner 须为现有成员
```

链上副作用（P0）：把**所有现有成员**的 `ChatHook` 场景授权从旧群主迁移到新群主
（成员 ↔ 群主 的可选 1:1 私聊授权）。客户端无需额外操作，但 UI 应据 `member_role_tag` 刷新
群主 / 管理员标识。群主退群前**必须先转让**（`MustTransferFirst`）。

---

## 8. 治理冻结 / Frozen groups

- `chat_isGroupFrozen(group_id)` 或 `group_mls_snapshot.frozen == true` 时，链拒绝
  `commit` / `anchor_message_digest` / `request_join`（报 `GroupFrozen`）；元数据仍可读。
- 客户端应展示「已冻结」只读态，禁用发起成员变更的入口（解散拆除期间也会临时置 frozen）。

---

## 9. 错误 → 客户端动作映射 / Error-to-action table

| 链上错误 | 含义 | 客户端动作 |
|---|---|---|
| `EpochStale` | 并发 commit 落败 | 重取快照 + 补齐 + 重算重试（§6） |
| `WelcomeMismatch` | welcome 与 added 不双射 | 修正：每个新成员恰一条非空 Welcome |
| `AddeeNotJoinable` | 被加者无 KeyPackage | 提示对方先 `publish_key_package` |
| `TwoMemberGroupForbidden` | 变更后恰 2 人 | 改为一次 ≥2 人增 / 减到 1 人；1:1 走链下 |
| `NotApproved` | 私群未批准即 Add | 先 `approve_join` |
| `Banned` | 目标被封禁 | 先 `unban_member` 或换人 |
| `WelcomeNotFound` | claim 时信箱空 | 多为「先 claim 后读」误用；检查顺序（§5） |
| `RateLimited` | 写入型 MLS 操作超频 | 退避后重试（窗口 `MlsActionWindow`） |
| `GroupFrozen` | 群被冻结 | 只读态，禁用变更入口 |
| `MustTransferFirst` | 群主未转让即退群 | 先 `transfer_ownership` |

---

## 10. 只读 RPC 速查 / Read-only RPC reference

| 方法 | 用途 |
|---|---|
| `chat_pendingWelcome(groupId, who, at?)` | 取回待领 Welcome（hex），**先读后 claim** |
| `chat_handshakeAtEpoch(groupId, epoch, at?)` | 指定 epoch 的 Commit 字节（离线补齐） |
| `chat_groupMlsSnapshot(groupId, at?)` | 群 MLS 锚点快照（epoch/hashes/cid/count/cipher/public/frozen） |
| `chat_isGroupFrozen(groupId, at?)` | 群是否冻结 |
| `chat_listConversations(who, at?)` | 统一会话切片（群在末尾，`last_active/unread=0`，需客户端 Merge） |

均只读免费（走 `runtime_api`，非交易）；`at` 省略取最佳区块。等价 `state_call` 形式见
`ChatGroupApi` / `ChatViewApi`。

---

## 11. 端到端最小时序 / Minimal end-to-end sequence

```text
A=owner, B/C = members

A: publish_key_package?              # 群主无需被加，可不发
B: publish_key_package(kpB)
C: publish_key_package(kpC)
A: create_group(cidA, suite, is_public=true, treeA, transA)         → gid, epoch 0
A: commit(gid, 0, commitAB C, tree1, trans1, cid1,
          welcomes=[(B,wB),(C,wC)], added=[B,C])                    → epoch 1, member_count 3
B: chat_pendingWelcome(gid,B)=wB → OpenMLS 处理 → claim_welcome(gid)
C: chat_pendingWelcome(gid,C)=wC → OpenMLS 处理 → claim_welcome(gid)
（此后人类消息全部走链下 MLS + relay，不再有任何链上消息 extrinsic）
A: set_group_profile / set_member_mute / ban_member …               # 应用层治理
A: transfer_ownership(gid, B)  或  disband_group(gid)
```
