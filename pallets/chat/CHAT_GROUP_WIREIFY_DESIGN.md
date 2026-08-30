# 群 Wire 化 · 落地设计开发文档（群聊从轨 A 单 leaf → 每设备独立 leaf）

> 状态：开发文档 · v1.2（待评审签字）· **群多设备 design of record**
> 内容完整度：设计（§1–§14）+ 开发级补充（§15 接口契约 / §16 错误边界 / §17 flag·迁移 / §18 权重基准 / §19 验收阈值 / §20 WBS）。
> 评审签字（开工前须补齐）：☐ 链 / pallet　☐ 客户端 / mls-wasm　☐ 安全　☐ 产品
> 链侧前置结论：**empty-`member_delta` 推进型 commit 已在链上验证可行**（`pallets/chat/group/src/tests.rs::same_account_empty_delta_commit_rekey_is_allowed`，全绿）——原 Gate-G1 不再是 Blocking 风险，详见 §4.1 / §11。
> 定位：把已上线、测试全绿的 **1:1 Wire 多 leaf** 模型推广到**群聊**，用「每设备一个独立 leaf」替换已废止的
> **轨 A 共享单 leaf 托管**。目标是**根除「只读设备不能发」整套状态机**（primary/secondary + 在线交接 + PIN 备份），
> 让每台设备都是群里的完整成员，像主流 IM 一样多端并发收发。本文给出**链改评估、客户端复用映射、关键流程、
> 成员准入（公私群 vs 加设备）、过渡期复杂度缓解、安全模型、实施计划与验收**。
>
> 关联文档：
> - `CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md`（多设备总纲 v4：密码学约束 / 废止轨 A / 路线 B 终态 / Gate-B）
> - `CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md`（1:1 Wire 闸一 CD 选举 / 闸二 relay CAS / §3.7–§3.9 加入与 E2EI）
> - `pallets/chat/sync/`（EISA 同步锚：通讯录 / 聊天记录 / 会话索引 IPFS 自愈）
> - 链上：`pallets/chat/group/src/lib.rs`（DS/AS：`commit(expected_epoch)` / `GroupMembers` / `KeyPackages`）
> - 客户端：`nexchat/src/mls/`（群链上成员/定序：`chainHandshake.ts` / `groupMemberFlow.ts` / `addMembersToGroupFlow.ts`；
>   **待退役轨 A 群侧**：`groupHandoffRuntime.ts` / `sendingAuthority.ts` / `signingPinBackup`；
>   1:1 Wire（复用至群）：`directWireSession.ts` / `directAccountCommitCoordinator.ts` / `directWireCommitExecutor.ts` /
>   `directCommitCoordination.ts` / `deviceLeafCredential.ts` / `followCommitGuard.ts` / `wireJoinPlan.ts`）

---

## 0. TL;DR

CN：群聊现在用**轨 A**——一个账户在群里只占**一个共享 leaf**，签名钥从 vault 里剔除，任一时刻**至多一台设备**持钥能发，
其余设备只读、需经**在线交接**或 **PIN 恢复**才能发。这正是你反复踩到的「只读设备不能发」bug 的根因。**群 Wire 化**改为
**每台设备在群里各占一个独立 leaf（私钥本设备自持、不托管）**：每台设备都是完整成员，**直接并发收发**，**无主/附之分、
无在线交接、无 PIN**；丢设备就 `Remove` 那个 leaf（**按设备 PCS 自愈，无需换助记词**）。代价是**群成员能看出你大约有几台
设备**（设备数隐私回退），以及**全新设备被加入前不能解实时群消息**（历史正文仍由 `K_archive` 助记词自愈）。

**关键结论：链改为零（已验证）。** 链上成员表 `GroupMembers` 按**账户**键、`commit(expected_epoch)` 已提供**全局防分叉全序**
（群因此**不需要** 1:1 当年的 relay `commit_slot` CAS）。新增设备 leaf 走**链下 relay `s:<account>` 自通道**交付 Welcome/KP
（复用 1:1 wire 的 `device_join_*` 机制），链上仅记一条 **`member_delta` 为空、仅推进 epoch+tree_hash 的 commit**。
这条「空增删推进 commit」**已被链上单测证明放行**（`same_account_empty_delta_commit_rekey_is_allowed`：epoch+1、成员数不变、
落 `HandshakeLog`），`ensure_welcomes_match_added` 在 added 空 + welcomes 空时通过——**故链侧无改动、无残留风险**，
剩余工作全在客户端 + relay。

EN: Groups today use **Track A** — one account holds a **single shared leaf**, the signing key is stripped from the vault,
and **at most one device** may send at a time; others are read-only and need an **online handoff** or **PIN restore**. That
is exactly the "read-only device can't send" trap. **Group Wire-ification** gives **each device its own leaf** (private key
device-held, never escrowed): every device is a full member, **sends concurrently**, with **no primary/secondary, no handoff,
no PIN**; losing a device just `Remove`s its leaf (**per-device PCS, no mnemonic rotation**). Costs: the group can **see how
many devices you have** (device-count privacy regression), and a **brand-new device cannot decrypt live group messages until
added** (history text still self-heals from `K_archive`).

**Key finding: zero chain change (verified).** On-chain `GroupMembers` is keyed **per-account** and `commit(expected_epoch)`
already gives a **global anti-fork total order** (so groups need **none** of the relay `commit_slot` CAS that 1:1 required).
A new device leaf is delivered **off-chain via the relay `s:<account>` self-channel** (reusing 1:1 wire's `device_join_*`),
with the on-chain side recording only an **epoch-advancing commit with an empty `member_delta`**. This empty-delta advancing
commit is **already proven accepted by an on-chain unit test** (`same_account_empty_delta_commit_rekey_is_allowed`: epoch+1,
membership unchanged, logged to `HandshakeLog`); `ensure_welcomes_match_added` passes when both added and welcomes are empty —
**so the chain side needs no change and carries no residual risk**; all remaining work is client + relay.

---

## 1. 目标与非目标

### 1.1 目标
- **群聊每台设备都能直接并发收发**，无主/附之分、无在线交接、无 PIN。
- **根除「只读设备不能发」状态机**：删除 `groupSendMode` 三态、`SendingAuthorityBanner`、`SendingKeyPanel` 恢复入口、
  `groupHandoffRuntime` / `sendingAuthority` / `signingPinBackup` 在群侧的依赖。
- **按设备 PCS 自愈**：丢/被攻陷设备 → `Remove` leaf → 该设备失未来群秘密，无需轮换助记词。
- **链改接近零**：复用 `commit(expected_epoch)` DS/AS，不动 `pallet-chat-group` 成员模型（见 §4 验证）。
- **历史可读凭助记词自愈**：沿用 EISA `K_archive`（与本方案正交）。

### 1.2 非目标
- **隐藏群内设备数**：每设备 leaf 必然对群成员暴露设备数；要"对外单一 leaf"是**路线 B 虚拟客户端**的目标（Gate-B 阻断，本方案不做）。
- **全新设备无 Add 即可发**：新设备被加入群前不能发/解实时消息（历史正文走 archive）；全账户离线后的重入需 §8.4 兜底。
- **群历史 epoch 协议级恢复**：与总纲一致，历史**可读正文**来自 archive，不来自 MLS 状态。
- **改 1:1**：1:1 已是 Wire 多 leaf，本方案只动群。

### 1.3 与多设备总纲的关系
- v4 总纲 [`CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md`](./CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md) 废止「群=轨 A ⊕ 1:1=Wire」混合方案。
- 本方案 = **群与 1:1 同构的 Wire 多 leaf**（「每设备 leaf + CD 选举 + staged commit」），
  差别仅在**定序后端**：1:1 靠 relay `commit_slot` CAS，**群靠链上 `expected_epoch`**。
- 是 Gate-B 未达期间的**现行目标**；终态路线 B 见总纲 §6（虚拟客户端，隐藏设备数）。

---

## 2. 背景：为什么切

轨 A 的「共享单 leaf + 签名钥剔除托管」是为了换「换机只读自愈 + 链改近零」，代价是三条已知局限（总纲 §1.1）：

| 局限 | 现象 | 牵出的机器 |
|---|---|---|
| 单活跃发送 | 任一时刻只有一台设备能发群消息 | `primary_device_id` + `HandoffReceipt` |
| 换机后发需交接 | 只读设备发不了，必须在线交接或 PIN | `groupHandoffRuntime` + `signingPinBackup` + 三态 UI |
| PCS 让渡 | 持助记词可解未来群消息，须如实披露 | `K_mls_escrow` vault + 披露 UI |

本次三台模拟器全部卡在「只读、互相都不是 primary、无人能授权、也没 PIN 备份」的死锁，正是这套机器的失败态。
**Wire 化把这三条局限连同其全部机器一起删除**，换来「群内设备数暴露 + 新设备需被 Add」两条新代价（§9）。

---

## 3. 模型：每设备 leaf 的群

```
账户 Alice（3 台设备）在群 g:42 里：
  轨 A（现状）：           Wire 化（本方案）：
  ┌───────────────┐       ┌───────────────────────────┐
  │ leaf: Alice   │       │ leaf: alice#devA  (本设备私钥) │
  │ (共享, 签名钥  │  →    │ leaf: alice#devB  (本设备私钥) │
  │  托管/剔除)    │       │ leaf: alice#devC  (本设备私钥) │
  └───────────────┘       └───────────────────────────┘
  对群只见 1 个成员         对群可见 3 个 leaf、同一 identity=Alice（E2EI 绑定）
  链上 GroupMembers: Alice  链上 GroupMembers: Alice（不变，仍 1 条/账户）
```

- **leaf 身份**：`deviceLeafIdentity = {account}#{deviceId}`（已落地，`directConv.ts` 的 `accountFromLeafIdentity` /
  `deviceFromLeafIdentity` 可从 leaf 反解账户/设备）。同账户多 leaf 的 `credential` 经 **E2EI 设备 leaf 凭证**
  （`deviceLeafCredential.ts`，账户 SS58 钥签稳定 leaf 钥、作自定义 leaf-node 扩展驻留 MLS 内）绑定到同一 `AccountId`。
- **私钥边界（安全红线）**：群设备 leaf 私钥**绝不**由 `vault_master` 派生、**绝不**进 vault——否则按设备 PCS 失效，
  退回轨 A 的 PCS 让渡。这与 1:1 Wire 的派生边界一致（`deviceLeafCredential.ts` / `devicePeerKey.ts` 现行实现）。
- **引擎**：群与 1:1 各用独立 `OpenMlsEngine` 实例（`signer: Option` 为 client 级）。Wire 化后**群引擎也持
  本设备 signer**（不再是只读托管引擎），所以 `canExportEscrow()` 恒真、`no_signer()` 路径在群侧消失。

---

## 4. 链上分析（关键：为什么链改接近零）

`pallet-chat-group` 的成员与定序模型（实测 `lib.rs`）：

| 机制 | 现状 | 对 Wire 化的影响 |
|---|---|---|
| `GroupMembers` | `StorageDoubleMap<GroupId, AccountId, GroupMember>`（**1 条/账户**，lib.rs:394） | **不变**：设备 leaf 是 MLS 树内部，账户仍 1 条成员记录 |
| `member_count` / `MaxGroupMembers` | 计**账户**数（lib.rs:875） | **不变**：设备 leaf 不计入账户成员数 |
| `commit(expected_epoch)` | 区块全序仲裁并发 commit、防分叉（lib.rs:824） | **直接复用**为群 Wire 的定序后端；**无需** relay `commit_slot` CAS |
| `KeyPackages` | `(AccountId, KeyPackageId)` 多 KP（lib.rs:338），Add 时消费 | 设备 KP **走链下 relay**（同 1:1 wire），不上链、不占链上 KP 池 |
| `welcomes` ↔ `member_delta.added` | `ensure_welcomes_match_added`：每个新增**账户**一条非空 Welcome；无增员则须为空（lib.rs:867） | 设备 Welcome 走链下 relay；added 空 + welcomes 空时该校验通过（已验证，见 §4.1） |
| `TwoMemberGroupForbidden` / `new_count != 2` | 计账户（lib.rs:882） | **不变**：设备 leaf 不触发 |
| `note_mls_action` 限频 | 每账户对 `commit`/`anchor` 限频（lib.rs:820） | 加/删设备消费同一限频预算 → 靠 §8 lazy+批处理缓解 |

### 4.1 设备 leaf 的加入：empty-`member_delta` 推进型 commit + 链下 Welcome

把"给已是成员的账户加一台设备 leaf"建模为：

1. 账户的**在线兄弟设备**（已是该群成员 leaf、经 CD 选举为协调者）对群引擎执行 `add_device`（staged commit），
   产出 `commit + welcome`。
2. **上链**：调用 `commit(group_id, expected_epoch, commit_bytes, new_tree_hash, new_transcript_hash, group_info_cid,
   welcomes=[], member_delta={added:[], removed:[]})`——**`member_delta` 为空**（没有新增/移除**账户**），链只推进 epoch
   并更新 tree/transcript/group_info 锚点。`ensure_welcomes_match_added` 在 added 为空、welcomes 为空时通过。
3. **链下**：新设备的 **Welcome + KeyPackage** 经 relay `s:<account>` 自通道交付（复用 1:1 wire 的
   `device_join_request/offer/kp` 三段式 + graft-Welcome 接收器）。新设备 `processWelcome` 追到当前 epoch → 成为合法 leaf。
4. 其它群成员 `processCommit` 跟进新 epoch（看到 Alice 多了一个 leaf，identity 仍 = Alice，由 E2EI 复验，
   见 `CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md` §3.9）。

> ✅ **链侧已验证（原 Gate-G1，现已清除）**：`commit` 接受「`member_delta` 为空但 `new_tree_hash` 变化」的推进型 commit——
> 链不解析 MLS 密文，只校验 epoch/delta/welcomes/hash。证据：`pallets/chat/group/src/tests.rs::same_account_empty_delta_commit_rekey_is_allowed`
> （ALICE 以「新设备」身份提交空增删 commit → epoch 1→2、成员数不变、落 `HandshakeLog`、committer 未被误判 `NotMember`，全绿），
> 边界用例 `true_non_member_commit_still_rejected` 证明仅真·跨账户非成员被拒。**故 §4.3 备选无需启用。**

### 4.2 设备 leaf 的移除

- 移除账户的**某一台**设备（账户仍有其它设备在群）：`remove_device` → 同样是 **`member_delta` 为空的推进型 commit**
  （账户没退群），Welcome 无关。被移除设备解不出新 epoch（按设备 PCS）。
- 移除账户的**最后一台**设备 = 账户退群：`member_delta.removed = [account]`，走现有链上踢人/退群路径（`groupMemberFlow.ts`），**无改动**。

### 4.3 备选（已不需要，仅存档）
> §4.1 链上验证已通过，本备选**不启用**，仅留作历史决策记录。
- 若链曾拒绝空-delta 推进 commit，则需在 `commit` 增加「设备级 leaf 变更」标记或允许 `member_delta` 携带"同账户设备增量"
  （不入 `GroupMembers`，仅放行 hash 推进）——有界小链改。**当前无需。**

---

## 5. 客户端复用映射（1:1 Wire → 群）

群 Wire 化的客户端骨架**绝大部分可从 1:1 Wire 平移**，差别集中在「定序后端」和「成员发现」：

| 能力 | 1:1 Wire 现有件 | 群 Wire 化做法 |
|---|---|---|
| 设备 leaf 身份 / 反解 | `directConv.ts`（`deviceLeafIdentity`/`accountFromLeafIdentity`） | **直接复用** |
| E2EI 设备 leaf 凭证 | `deviceLeafCredential.ts`（`verifyLeafKeyBinding`）+ wasm `setLeafBinding`/`keyPackageBinding` | **直接复用** |
| staged commit 原语 | wasm `add_device`/`remove_device`/`selfUpdate`/`mergePending`/`clearPending` | **直接复用**（群引擎调用） |
| CD 选举（防账户内双 commit） | `directAccountCommitCoordinator.ts` + `directCommitCoordination.ts`（presence/intent/election） | **复用选举与 presence**；意图类型增加群设备增删 |
| staged 执行器 | `directWireCommitExecutor.ts`（`createAddDeviceExecutor`：runIntent→staged、commitAccepted→merge） | **复用模式**，新建 `createGroupDeviceExecutor`：merge 前先上链 `commit` |
| 设备加入三段式 + graft Welcome | `device_join_request/offer/kp` + `directWireSession.ts` | **复用**：Welcome/KP 走 `s:<account>` 链下 |
| 成员侧复验 add commit | `followCommitGuard.ts`（`verifyIncomingCommit`/`inspectCommitBindings`） | **复用**：群成员对收到的设备 add commit 复验 E2EI 绑定 |
| 设备名册 / 披露 UI | `wireDeviceRoster.ts`（`computeWireDeviceRoster`）+ `WireDeviceSheet` | **复用**：群侧也展示「N 台设备 / 移除我的设备」 |
| **定序后端** | relay `commit_slot` CAS（`commit_slot.rs`，闸二） | **不用**；改用**链上 `commit(expected_epoch)`** 作 CAS |
| **成员发现 / Add 新账户** | 链下 pairwise | **保留轨 A 群既有链上路径**（`chainHandshake.ts` / `addMembersToGroupFlow.ts` / `groupMemberFlow.ts`） |

> 要点：**群 Wire 化 = 「轨 A 群的链上成员/定序骨架」+「1:1 Wire 的每设备 leaf/CD/staged/E2EI 骨架」的拼接**。
> 不是新造系统，是把两套已落地能力对接。

---

## 6. 关键流程

### 6.1 建群（owner 首发）
沿用 `chainHandshake.ts` owner 主导流程，唯一变化：owner 用**本设备 leaf**（持 signer）建群，不再做轨 A 的"共享 leaf + 托管"。

### 6.2 加成员（新账户）— 链上规则不变

**不变**：走链上 `commit` + `member_delta.added=[新账户]` + 链上 Welcome（`addMembersToGroupFlow.ts`）。这是**账户级**，与设备 leaf 无关。

| 群类型 | 新账户如何进来 | 链上校验 |
|---|---|---|
| **私有群** `is_public=false` | 账户 `request_join` → 群主/管理员 `approve_join` → Add commit | Add 时须有 `JoinApprovals`，否则 `NotApproved` |
| **公开群** `is_public=true` | 不走审批；被加账户须已发布 KeyPackage（opt-in）→ 管理员 Add | 无 `JoinApprovals`；仍须 `AddeeNotJoinable` 防护 |

共同点：执行 Add 的 committer 须为群主/管理员；人数上限公私群相同（`MaxGroupMembers=500`，计**账户**；禁 2 人群）。

> **与 §6.3 区分**：上表是「新**账户**进群」；已是成员的账户加**新设备**不走审批，见下节。

### 6.3 加设备（同账户新设备）— 本方案核心新增

新设备不在 MLS 树里 → 须 **Add** 成 leaf 后才能解/发实时群消息。流程（复用 1:1 `device_join_*`，定序改链上）：

```text
1. 新设备 broadcast device_join_request { device_id }  on s:<account>
2. CD 回 device_join_offer { conv_ids: [活跃群 g:…] }   // 延迟 Add：仅活跃群
3. 新设备回 device_join_kp { kps per conv }             // 含 E2EI 设备 leaf 凭证
4. CD 每群：add_device staged → 上链 commit(empty member_delta) → merge
5. Welcome 经 s:<account> 链下给新设备 → processWelcome → 可收发
6. 其它成员 processCommit + verifyIncomingCommit（E2EI）
```

**不需群主审批**——该**账户**已是成员；安全靠 E2EI（防冒充设备）+ 成员侧复验。

| 情形 | 谁 Add | 用户感知 |
|---|---|---|
| 我有兄弟设备在线 | CD 自动 | 登录后数秒可发 |
| 我全离线、群成员在线 | peer-add-device | 稍等 |
| 全离线 | 等上线 / External Commit | 可读 archive，发需等待 |

链上细节见 §4.1（empty-`member_delta` 推进 commit + 链下 Welcome）。

### 6.4 删设备 / 丢设备
见 §4.2：`remove_device` → empty-delta 推进 commit；被移除设备失未来群秘密（按设备 PCS）。UI 复用 `WireDeviceSheet` 的"移除我的设备"。

### 6.5 换机
- 新设备解锁（持助记词）：**立刻**从 `K_archive` 读全部历史正文（§EISA，与 MLS 无关）。
- 被加入各活跃群（§6.3）后，从该 epoch 起解**新**群消息。
- 中间空窗：relay backlog + 最终一致补齐（`MsgArchiveSync.scheduleGapRefill`，已落地；群 Wire 嫁接 Welcome 落地后同样触发）。

---

## 7. 并发与定序

- **账户内**（我两台设备同时想改群）：**CD 选举**串行化（复用 `directCommitCoordination` 选举：最小 DeviceId / 在线最久），
  只有协调者发 commit，其余等待 → 避免同账户双 commit。
- **账户间**（我加设备 与 别人加成员 撞同一 epoch）：**链上 `expected_epoch` 闸门**裁决——区块全序，落败方 `EpochStale`，
  重取群状态、追平 epoch、重提（`syncGroupEpoch` 已有）。**这正是群比 1:1 简单的地方**：1:1 无链上全序才需要 relay `commit_slot` CAS；
  群天然有，**relay 串行化件在群侧完全不需要**。
- staged 语义：commit 先 staged、**上链 `commit` 成功后才 `mergePending`**（失败则 `clearPending` 重来），杜绝"本地已推进、链上落败"的分叉。

### 7.1 链 CAS 语义（实现核对，G3）

读 `pallets/chat/group/src/lib.rs::commit` 确认：

- **CAS 仅看 epoch**：唯一防分叉闸门是 `ensure!(g.epoch == expected_epoch, EpochStale)`（L824）。`new_tree_hash`/
  `new_transcript_hash` **不被链校验**，仅作为不透明承诺写入群状态（L921–922）。→ 定序正确性只依赖 `expected_epoch`，
  执行器的 `preEpoch` 已提供；驱动只需把 `EpochStale` 映射为「追平 + 重提」。
- **空-delta 设备 commit 无需群主**：`member_delta.added` 为空且未触及他人时 `changes_others=false`，跳过 Owner/Admin
  角色校验（L834–843），仅需 committer 是成员（L826）。即**任一登录设备**可推空-delta rekey/设备 commit
  （与 `same_account_empty_delta_commit_rekey_is_allowed` 一致）。Welcome 走链下 `s:<account>`，链上 `welcomes=[]`。

### 7.2 staged 后置 fingerprint（G3b · ✅ 已落地，方案 A）

**背景**：`mls-wasm` 的 `addMembersStaged` 等返回的 `tree_hash`/`transcript_hash` 来自 `fingerprint(group)`，该 fingerprint 用
**自定义口径**（`tree_hash=SHA256(导出棘轮树)`、`transcript_hash=epoch_authenticator`），二者在**合并前**仍是**旧 epoch**值
（OpenMLS 未合并 pending commit 时 `export_ratchet_tree`/`epoch_authenticator` 反映旧态）。1:1 Wire 不受影响（其 commit 经
relay，不携带这两个 hash）。**群侧上链需要后置（新 epoch）hash**。

**关键约束**：自定义口径的 `transcript_hash=epoch_authenticator` **只在 merge 后**才可得（属新 epoch 密钥调度），故"用 staged
context 复算自定义口径"不可行。OpenMLS 仅 `StagedCommit::group_context()` 为 public（`export_group_context()`/`tree_hash()` 受
`test-utils` feature 门控，不可用于产物构建）。

**落地（方案 A）**：新增 wasm 原语 `stagedCommitFingerprint(convKey)`，从 pending `StagedCommit::group_context()` 读 **MLS 原生**
后置承诺 `(tree_hash, confirmed_transcript_hash, epoch)`——这正是 `merge` 将安装的 context，故为所有应用该 commit 的成员共享的
**真实**后置承诺。口径决策：

- 群 Wire **设备 commit 走 staged → 上链路径**，其上链 hash 一律取自 `stagedCommitFingerprint`（MLS 原生口径），
  add/remove/rekey 统一。`GroupWireSession` 的 `submitGroupCommit`（G3c）在 executor 暂存后、提交 `expected_epoch` CAS 前调用。
- `fingerprint(group)`（建群 epoch-0 + 轨 A 合并路径）**保持自定义口径不变**（零风险，既有测试与 1:1/轨 A 流程不动）。
- 口径不一致**无害**：链按不透明存储、不复验密码学（§7.1），且代码无任何处比对 hash 值（仅断言 32 字节）；收敛由 commit
  字节驱动。MLS 原生 `confirmed_transcript_hash` 对 SHA256 套件亦为 32 字节。

**验证**：`mls-wasm/tests/staged_fingerprint.rs`（原生：暂存 fp = pre+1、暂存不推进实时 epoch、merge 落到暂存 epoch、逐 epoch
推进、32 字节）+ `src/mls/stagedCommitFingerprint.test.ts`（TS：≥3 账户群 staged add 的字段映射、对端收敛、merge 落点、无暂存
抛错）。TS 包装 `OpenMlsEngine.stagedCommitFingerprintByConv` 做 snake→camel 字段映射 + epoch BigInt→number。

> 方案 B（merge-then-submit + `gwire:` 快照回滚）已弃用：实现重、回滚易错，且方案 A 无需投机合并即可拿到真实后置承诺。

---

## 8. 过渡期复杂度缓解（Gate-B 未达期间）

每加/删一设备要在其所在的每个群发 commit（fan-out = O(群数)），KP 随设备增多。四个叠加手段把它压到可接受：

### 8.1 延迟 / 按需 Add（最有效）✅ 已落地（G4）
新设备**不**急着进所有群，只在群**变活跃时**（打开 / 发言）才 `add_device`；休眠/归档群不加，历史从 archive 读。
→ 实际 fan-out 降到「真正在用的活跃群」，后台异步、用户无感。

**落地**：
- 纯规划器 `src/mls/wireGroupJoinPlan.ts`（`planWireGroupJoin`，仿 `wireJoinPlan.ts`）：把 CD `device_join_offer`
  提供的群按 `isActive`（活跃度）+ `isHeld`（是否已是该群 leaf）切分为 `joinNow`（活跃且未持有 → 现在嫁接）/
  `defer`（休眠且未持有 → 延迟）；保序、去重、忽略非 `g:`；缺 `isActive` 时退化为急加载（向后兼容）。纯函数 → 可单测
  （`wireGroupJoinPlan.test.ts`，4 用例）。
- `GroupWireSession`：`onDeviceJoinOffer` 改为消费该计划——只为 `joinNow` 群铸造 KeyPackage 并请求嫁接，`defer` 群记入
  `deferredGroups`；新增 `activateGroup(conv)`：休眠群变活跃（打开/发言）时仅为该群铸造 KP + 定向嫁接（已持有 / 未延迟则
  空操作）。新增可选 deps `isGroupActive` / `onJoinPlanned`。端到端验证见 `groupWireSession.test.ts`「§8.1 lazy Add」：
  休眠 offer 不发 KP、链 epoch 不动 → `activateGroup` 后单群嫁接 + 收敛 + 二次激活幂等。
- **appStore 实时接线（G7+）**：`isGroupActive` 读 `wireGroupActivity.ts`（当前打开 **或** 7 天内 `recency` 且未归档 → 活跃）；
  `openConversation` / `prepareGroupWireConversation` / 入站 stale-cipher 恢复路径在群 wire 模式下先 `activateGroup` 再 `ensureGroupMlsReady`；
  Welcome 落地经 `onGroupGrafted` bump `mlsSyncRev` 并刷新当前会话；`wireGroupActivity.test.ts`（4 测）。

### 8.2 CD 批处理 / 合并提案 ⏳ 待做（G4 范围外，留 G5/G6）
CD 统一发设备变更；MLS 单 commit 可携带**多个 proposal**，把临近的多次设备增删合并进一个 commit。减少 commit 数 + 防撞 epoch。
> 需新增 wasm 多提案暂存原语（单 staged commit 内聚合多个 add/remove）+ 引擎/驱动批窗口；属较大密码核改动，与 §8.1 正交，
> 推迟到 G5/G6 与 CD 选举/调度一并做。

### 8.3 KeyPackage 治理（部分已就位）
- 设备 KP **走 relay-only**（同 1:1 wire），不上链 → 无链上膨胀/押金（对比 `chainKeyPackage.ts` 的链上 KP 仅留给"新账户入群引导"）。
  **现状**：群 Wire join 握手（`device_join_kp`）已是 relay-only（KP 经 `s:<account>` 交付，从不上链）→ 此项**已就位**。
- **last-resort KP** + 每设备小池子，避免一次性 KP 耗尽；陈旧 KP 定期 GC。⏳ 待做：需 KP 池/存储；与 §8.2 一并推到 G5/G6。
- KP 量级 = O(设备数) 小常数，非 O(群数)。

### 8.4 无兄弟在线 / 全设备灭失的兜底
- 有别的群成员在线：**peer-add-device** ✅ 已落地（G5）——某成员/管理员用我的设备 KP（relay 交付）+ **E2EI 凭证**校验"该 KP 属于我"后，
  代我 `add_device`（群版的 1:1 对端代 Add，见 `CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md` §3.8）。
  **落地**（`GroupWireSession`）：
  - 请求侧 `requestGroupPeerAdd(g:<id>)`：造携带本设备 MLS 内 E2EI 绑定的一次性 KeyPackage、登记待嫁接、广播认证 `peer_add_req`；
    武装冷启动回退（窗口内无人嫁接 → `onPeerAddTimeout`，宿主可等待 / 回退 External Commit）。
  - 成员侧 `onPeerAddReq`：提交前 relay-/链-trustless 五连鉴权——(a) relay 盖章发送者 == 声称请求方（防冒充）、(b) 请求方非我、
    (c) 我确实持有该群、**(d) 请求方账户已是群当前成员**（`isGroupMember`，**fail-closed**：只接既有成员的新设备，严禁夹带新账户）、
    (e) KeyPackage 携带由请求方账户钥签名的**有效** MLS 内绑定（§6.4）；再加幂等去重（`memberIdentities` 已含该 leaf 则跳过，
    真并发由链 `expected_epoch` CAS 兜底）。通过后经链定序 `add_device`，Welcome 经 `s:<请求方账户>` 投递。
  - 验证：`groupWireSession.test.ts`「peer-add fallback (§8.4)」——成员代嫁接既有成员新设备（验证→链 add_device→Welcome→入群→跨端解密）、
    拒绝**非成员**账户（闸 d）、拒绝**伪造绑定**（闸 e），用真实 SS58 钥 + 真实定时器；**`onPeerAddTimeout` 冷启动回退** +
    **`ensureGraftOrPeerAdd`**（非 CD 延迟群 → peer_add_req，幂等）单测。
  - **appStore 接线（G7+）**：无兄弟 join 安定经 `planWireGroupJoinSettle` → 活跃成员群 `requestGroupPeerAdd`；`onPeerAddTimeout` 重广播
    `peer_add_req`；打开/入站恢复经 `ensureGraftOrPeerAdd`；群 Welcome 落地触发 `scheduleMsgArchiveGapRefill`（§4.5 与 1:1 对齐）。
- 无人在线：落到 **External Commit 自助入群**（总纲 §10.2 / `CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md`，需放开 External Commit，
  与本方案正交的可选能力）或等兄弟上线。⏳ External Commit 仍为可选项、未在 G5 落地（`onPeerAddTimeout` 已留出宿主回退钩子）。

---

## 9. 安全模型

| 威胁 / 维度 | 轨 A 群（现状） | Wire 化群（本方案） |
|---|---|---|
| 助记词泄漏 → 读未来群消息 | **可**（PCS 让渡，须披露） | **不可**（设备 leaf 私钥不托管，保 PCS） |
| 助记词泄漏 → 冒名发送 | 不可（签名钥不托管） | 不可（设备 leaf 签名钥本设备独有） |
| 移除被攻陷设备 | 不自愈（需轮换信任根） | **按设备 PCS 自愈**：`Remove` leaf 即失未来秘密，无需换助记词 |
| 群内设备数隐私 | **不暴露**（单共享 leaf） | **暴露**（群成员可见 N leaf）——须 UI 如实披露 |
| 恶意设备混入 | 轨 A 无子群审批 | 需 **E2EI 设备 leaf 凭证**（SERIALIZATION_SPEC §3.9）+ 成员侧 `verifyIncomingCommit` 复验；peer-add-device 尤需校验 identity==该账户 |
| 单点故障（无人能发） | **有**（无 primary 在线即死锁，本次 bug） | **无**（每设备独立可发） |

**产品须如实披露**：群内多端可并发收发、移除设备即自愈；但**群成员能看到你大约有几台设备**，新设备"能在群里发"需先被加入。
UI 复用 1:1 的 `WireDeviceSheet` + `ChatWindow` 顶栏披露副标题，群侧加同款"N 台设备 / ✓ 已验证 / 移除我的设备"。

> **UI 落地状态（G6）**：✅ 群设备**披露**已落地——纯名册 `computeWireGroupRoster`（`wireDeviceRoster.ts`，self/members/去重账户数，单测 `wireDeviceRoster.test.ts`）+ `appStore.wireGroupRosterFor`（门控 `wireGroupMultileafEnabled`，读 `openMlsEngine.memberIdentities(g:<id>)`，捕获 `groupWireDeviceId` 标记本机）+ `ChatWindow` 顶栏「端到端加密 · N 台设备」+ 🛡 角标 + `WireGroupDeviceSheet`（复用 `wx-device-*` 样式）。
> ✅ 群「移除我的设备」按钮**已点亮（G7，flag 门控）**：`appStore` 在 `useChainCp && wireGroupMultileafEnabled` 下实例化实时 `GroupWireSession`（engine=群 `openMlsEngine`、chain=`chainClient` 满足 `GroupCommitChain`、`syncGroupEpoch` 复用 `groupMemberFlow.syncGroupEpoch` 链回放、`isGroupMember` 读本地 `memberIdentities`），导出 `removeGroupWireDevice(g:<id>, identity)`（仅可移除自己非本机 leaf，CD 上链 / 非 CD 委派），`WireGroupDeviceSheet` 经 `onRemove` 接入并显示「移除」按钮（其行为由 G7 验收矩阵 `groupWireAcceptance.test.ts`「移除设备自愈」场景证明）。默认 flag OFF → 生产路径零影响。

---

## 10. 与轨 A / 路线 B 的关系与迁移

- **删除轨 A 群侧**：`groupHandoffRuntime` / `sendingAuthority` / `signingPinBackup` / `groupSendMode` 三态 / 群 `SendingAuthorityBanner`
  / 群 vault 托管（`exportEscrowState` 群路径）在群侧退役。**保留** 1:1 wire 与 EISA（archive/contacts/index）不动。
  > **退役落地状态（G6）**：采用**flag 门控退役**而非硬删（`wireGroupMultileafEnabled` 默认 OFF 时轨 A 群侧完全不变；ON 时群侧轨 A 面失效）。
  > 引擎层已由 G1 跳过（vault cold-start / `GroupHandoffRuntime` / vault backup 在 ON 时不启动 → `groupSendMode` 恒 `primary`）。
  > G6 再补 UI 层显式门控：`ChatWindow` 群只读发送阻断、`SendingAuthorityBanner`、`NewGroupChat` 建群只读阻断均在 `wireGroupMultileafEnabled` 时退役。
  > 旧 Track A 代码（handoff/PIN/vault crate）**保留**于 `mlsVaultEnabled` 路径下服务非 wire 用户，不在 G6 物理删除（避免破坏现网默认路径；Gate-B 转绿后随路线 B 一并清理）。
- **可选共存（不推荐）**：保留群 vault 仅作"全新设备被 Add 前只读引导"兜底——与 Wire 发送模型冲突，**不建议**。
- **迁路线 B**：Gate-B 转绿后，群与 1:1 一起从「多 leaf」收敛为「设备子群虚拟客户端」——群内从"可见 N 设备"变"无感单一 leaf"，
  并发由 `reuse_guard` PRP 原生支持，加/删设备 fan-out 从 O(活跃群) 降到 O(1)（只动设备子群），同时**修复设备数隐私回退**。
  迁移：会话内成员模型切换 + 各群重 rekey 一次；archive/contacts 无需迁移（正交）。

---

## 11. 实施计划

> 链侧前置（原 Gate-G1）已验证通过（§4.1），G0 仅剩 wasm 侧 spike。先 spike 后排期。

| 阶段 | 交付 | 依赖 | Gate / 状态 |
|---|---|---|---|
| **链侧前置** | empty-`member_delta` 推进型 commit 链上放行 | `pallet-chat-group` | ✅ **已验证**（`same_account_empty_delta_commit_rekey_is_allowed`） |
| **G0 · Spike（Blocking）** | 群引擎同账户多 leaf：① 并发收发 + 互读；② 按设备 PCS；③ 设备 Welcome/KP 走 `s:<account>` 链下并入树 | OpenMLS 0.8.1 | ✅ **已完成**（`mls-wasm/tests/group_wire_spike.rs`，S1–S3 全绿） |
| **G1 · 群引擎 Wire 化** | 群引擎持本设备 signer（弃只读托管）；`deviceLeafIdentity` 群侧建群/入群；E2EI 凭证嵌入群 KP | §3 / `deviceLeafCredential.ts` | ✅ **已完成**：`config.wireGroupMultileafEnabled`（默认 OFF）；`appStore` 群引擎 Wire 模式 init（设备 identity + E2EI 绑定 + 跳过轨 A 托管/交接/备份）；`groupWireE2ei.test.ts` 验证群 KP 绑定可验 + 伪造拒绝（全绿） |
| **G2 · 加/删设备（核心）** | `createGroupDeviceExecutor`（staged→上链 commit→merge）；`s:<account>` 设备 Welcome/KP；成员侧 `verifyIncomingCommit` 复验 | §4/§6.3/§6.4 | ✅ **已完成（解耦件）**：`groupDeviceCommitExecutor.ts`（6 测）、`device_join` `scope`（`directCommitCoordination.ts` + 测）、成员侧 `verifyIncomingGroupCommit`（`followCommitGuard.ts` 策略化 + `groupFollowCommitGuard.test.ts` 3 测）。群会话编排器（`GroupWireSession`）与 CD/链定序耦合，并入 G3 |
| **G3 · CD 选举 + 链上定序对接 + 群会话编排** | 复用 `directAccountCommitCoordinator` 选举；定序后端换成链上 `expected_epoch`（落败 `EpochStale`→`syncGroupEpoch`→重提）；`GroupWireSession`（接 executor + 成员侧复验 + `s:<account>` 设备加入握手） | §6.3/§7 | ✅ **已完成**：G3a 定序驱动 `groupCommitOrderingDriver.ts`（4 测）；链 CAS 语义已核对（§7.1）；G3b wasm 原语 `stagedCommitFingerprint` + TS 包装（方案 A，§7.2，原生/TS 测）；**G3c**：实链 `chainSubmitGroupCommit.ts`（G3b 后置哈希 + 空-delta + EpochStale 映射，6 测）+ `GroupWireSession.ts`（无 executor 协调器，`onExecuteIntent`→链定序驱动；group-scope 设备 join 握手；成员侧 `verifyIncomingGroupCommit`）+ 集成测试 `groupWireSession.test.ts`（announce→offer→kp→链 add_device→merge→Welcome→入群 / CD rekey / EpochStale 追平，3 测）。**剩**：对真实节点的 `scripts/e2e` 联调归入 G7 |
| **G4 · 复杂度缓解** | `wireGroupJoinPlan` 延迟 Add 调度；CD 批处理；relay-only + last-resort KP + GC | §8 | ✅ **§8.1 已完成**：纯规划器 `wireGroupJoinPlan.ts`（`planWireGroupJoin`，活跃→`joinNow`/休眠→`defer`，4 测）+ 接入 `GroupWireSession`（`onDeviceJoinOffer` 消费计划 + `deferredGroups` + `activateGroup` 按需嫁接 + `isGroupActive`/`onJoinPlanned` deps，`groupWireSession.test.ts`「§8.1 lazy Add」端到端）。relay-only KP（§8.3）已就位。**剩**：§8.2 CD 批处理（多 proposal）+ §8.3 last-resort KP 池/GC 推到 G5/G6（需 wasm 多提案原语 / KP 存储） |
| **G5 · 兜底路径** | peer-add-device（群版，见 SERIALIZATION_SPEC §3.8：E2EI + relay 盖章 + 接收方校验）；可选 External Commit | §8.4 | ✅ **peer-add 已完成**：`GroupWireSession.requestGroupPeerAdd`（请求侧：E2EI KP + `peer_add_req` + `onPeerAddTimeout` 冷启动回退）+ `onPeerAddReq`（成员侧五连鉴权：盖章发送者==请求方 / 非我 / 持群 / **请求方已是成员 fail-closed** / **有效 E2EI 绑定** + `memberIdentities` 幂等去重）；Welcome 经 `s:<account>` 投递、定序走链 CAS。`groupWireSession.test.ts`「peer-add fallback」3 测（happy / 非成员拒绝 / 伪造绑定拒绝，真实 SS58 钥）。**剩**：External Commit 自助入群（正交可选，未落地；已留 `onPeerAddTimeout` 宿主钩子） |
| **G6 · 轨 A 群侧退役 + UI** | 删除 `groupSendMode`/handoff/PIN 群依赖；群侧设备数披露 + `WireDeviceSheet` | §9/§10 | ✅ **披露 + flag 门控退役已完成**：群设备披露 `computeWireGroupRoster`（`wireDeviceRoster.ts` + 单测）+ `wireGroupRosterFor`（`groupWireDeviceId` 捕获）+ `ChatWindow`（「N 台设备」副标题 + 🛡 角标 + `WireGroupDeviceSheet` 仅披露）；轨 A 群侧在 `wireGroupMultileafEnabled` 时退役（`ChatWindow` 发送阻断 / `SendingAuthorityBanner` / `NewGroupChat` 建群阻断显式门控；引擎层 G1 已跳过 handoff/vault）。全套件 464 测绿、tsc 干净。**剩**：群「移除我的设备」按钮 + `GroupWireSession` appStore 实时会话接线随 **G7** 落地（需真实链）；轨 A crate 物理删除待 Gate-B/路线 B |
| **G7 · E2E 验收** | 多端并发群发 / 换机（archive 历史 + 被 Add 续发）/ 移除设备自愈 / 并发 commit 链上仲裁 | §13 | ✅ **客户端验收矩阵已完成**：`groupWireAcceptance.test.ts`（会话层 WASM 端到端，4 场景绿——多端并发群发 + 换机续发 / 移除设备 PCS 自愈 / 并发 `EpochStale` 仲裁追平 / 无主委派+CD 重选）；链侧仲裁由 pallet 既有用例覆盖。全套件 468 测绿、tsc 干净。**relay 路由前置已落地**（`relay-rs/server/src/protocol.rs`：`device_join_*` 走 `s:`；群 Welcome 按 `toAddr` 经 `g:` 路由入信箱；群 `peer_add_req` 盖认证发送者后广播 —— +2 单测，relay-rs 15 测绿；链改仍为零）。**`appStore` 实时会话已落地（flag 门控）**：`useChainCp && wireGroupMultileafEnabled` 下实例化 `GroupWireSession`（engine=群引擎、chain=`chainClient`、`syncGroupEpoch`=链回放、`isGroupMember`=本地名册），`start()`+`announceJoin()`+`onConnect` 重连；导出 `removeGroupWireDevice` 并点亮群设备面板「移除」按钮——tsc/lint 干净、全套件 468 测绿、默认 flag OFF 零生产影响。**剩**（节点依赖，无节点环境无法验证）：起真实 `nexus-node` 做多端联调（并发群发 / 换机被 Add / 移除自愈在真链上的端到端时序）；`scripts/e2e` 为链上 pallet harness，无 MLS 客户端层 |

### 11.1 最小 Spike 清单（G0，落到 `nexchat/mls-wasm/tests/`）
- **S1**（wasm）建群（≥3 账户）→ Alice 设备 A 建、设备 B `add_device` → 断言群内 Alice 有 2 leaf、identity 相等。
- **S2**（wasm）A、B 同 epoch 各发 N 条 → 断言 `(key,nonce)` 互不重用（每设备独立 ratchet）。
- **S3**（wasm）A `remove_device(B)` → B 解不出移除后新消息（按设备 PCS）。

> **链侧 S4/S5 已绿（无需重做）**：S4「空-delta 推进 commit 被接受、epoch+1」= `same_account_empty_delta_commit_rekey_is_allowed`；
> S5「并发 commit `expected_epoch` 仲裁、`EpochStale` 落败重试」由 `commit` 既有 epoch 闸门 + 既有并发用例覆盖。G3 落地时补一条端到端回归即可。

**Go/No-Go**：S1/S2/S3 全绿 ⇒ 立项（链侧前置已通过）。S2 若实测 nonce 重用 ⇒ 回退（等路线 B）。

---

## 12. 开放决策与风险

| # | 开放项 | 影响 | 建议 |
|---|---|---|---|
| O1 | ~~Gate-G1：链是否接受空-`member_delta` 推进 commit~~ | ~~链改是否真为零~~ | ✅ **已关闭**：链上单测 `same_account_empty_delta_commit_rekey_is_allowed` 证明放行，链改为零 |
| O2 | MLS 树规模 = Σ(账户×设备) | 大群 × 多设备时树变大、commit/解密变慢 | 延迟 Add（§8.1）控制活跃 leaf 数；监控树高 |
| O3 | `note_mls_action` 限频 | 加设备 fan-out 撞限频 | CD 批处理（§8.2）+ 延迟 Add；必要时调限频参数 |
| O4 | peer-add-device 的跨账户注入 | 安全（别人代加我设备） | 强制 E2EI 凭证 + relay 盖章 + 接收方三连校验（复用 1:1，见 SERIALIZATION_SPEC §3.8/§3.9） |
| O5 | 设备数隐私回退 | 群成员可见设备数 | 产品披露；终态靠路线 B 修复（§10） |
| O6 | 全设备灭失重入群 | 可用性 | peer-add-device / External Commit（§8.4），物理边界如实告知 |

---

## 13. 测试与验收

- **wasm 向量**：同账户多 leaf 并发无 nonce 重用；按设备 PCS；E2EI 设备凭证往返（复用 `deviceLeafCredential.test.ts` 模式）。
- **链上**（已绿，回归）：空-delta 推进 commit 接受（`same_account_empty_delta_commit_rekey_is_allowed`）；并发 commit `expected_epoch` 仲裁（既有 epoch 闸门）；账户成员数不因设备增减变化。
- **客户端**（✅ **G7 已落地**：会话层 WASM 端到端验收矩阵 `nexchat/src/mls/groupWireAcceptance.test.ts`，4 场景绿；用真实 `GroupWireSession` + 真实 `OpenMlsEngine` + 真实链定序驱动 + `createChainSubmitGroupCommit`，对**假**链 `expected_epoch` CAS + 内存 relay 总线）：
  - 多端并发群发：两设备对同一群在**同一 epoch** 各发，群内其它成员 + 本账户它机全解、按设备独立 ratchet（密文不同、无 key/nonce 重用）；
  - 换机：被 Add 后续发（设备-join 级联 → 新设备入群即发，成员可解）；archive 历史可读属持久化层（沿用 1:1 §4.5 补齐）；
  - 移除设备自愈：CD `removeDevice` 兄弟设备 → 被移除 leaf 解不出**后续**群消息（按设备 PCS），其余成员保持收敛；
  - 并发 commit 链上仲裁：并发赢家迫使 `EpochStale` → 落败方 `syncGroupEpoch` 追平、重 stage、链接受重试 → 最终一致；
  - 无主死锁**不复现**：非 CD 设备发起变更 → 委派给当选 CD 执行；CD 离线后兄弟**重选**为 CD 并自行 commit（无单点发送）。
- **relay 路由（✅ 已落地，`relay-rs/server/src/protocol.rs`）**：群 Wire 走**真实 relay** 所需的控制面路由扩展已补齐（此前被 vitest 的 fake `Bus` 广播屏蔽）——(1) `device_join_request|offer|kp` 加入 `s:<account>` 的 store/route/dedup 列表（1:1 + 群 Wire 设备加入级联共用）；(2) 群设备 Welcome 按 `toAddr` 经 `g:` 路由到加入设备账户并入信箱（`mls_control_recipient` g: 分支，与 1:1 `d:` welcome 对齐）；(3) 群 `peer_add_req`（§8.4）盖**认证发送者** `_senderAccount` 后广播给已连接账户，接收侧 5 闸鉴权过滤非成员/冒充/伪造绑定。新增 2 条单测（`device_join_cascade_routes_and_dedups_per_device`、`group_welcome_routes_to_joining_account`），relay-rs 全工作区 15 测绿。**链改仍为零**——群 commit 定序走链上 `chatGroup.commit`，relay `commit_slot` CAS 是 1:1 专用，群不碰。
- **appStore 实时接线（✅ 已落地，flag 门控，`appStore.ts`）**：`useChainCp && wireGroupMultileafEnabled` 下实例化 `GroupWireSession`（engine=群 `openMlsEngine`、relay=`relayClient`、chain=`chainClient`〔满足 `GroupCommitChain`：`signAndSendDev`+`groupSnapshot`〕、`syncGroupEpoch`=`groupMemberFlow.syncGroupEpoch` 链回放、`isGroupMember`=本地 `memberIdentities`），`start()`+`announceJoin()`+`onConnect` 重连重发 presence；导出 `removeGroupWireDevice(g:<id>, identity)`（仅移除自己非本机 leaf），`WireGroupDeviceSheet` 经 `onRemove` 接入并点亮「移除」按钮。tsc/lint 干净、全套件 468 测绿、默认 flag OFF。
- **E2E（真实节点，待联调）**：现有 `scripts/e2e/` runner（`scripts/docs/NEXUS_TEST_PLAN.md`）仅覆盖**链上 pallet** smoke（entity/commission…），**无** MLS/relay 客户端 harness；群 Wire 客户端行为在本仓一律走 WASM vitest（见上）。剩余真实节点联调（起 `nexus-node` 多端验证并发群发 / 换机被 Add / 移除自愈在真链上的端到端时序）属节点依赖步骤，本轮未在无节点环境执行。relay 路由 + appStore 实时会话前置均已就绪（见上）。
- **自愈断言**：仅持助记词 → 历史可读（archive）；群内"能发"需被 Add（有兄弟在线时秒级）。

---

## 14. 一句话结论

**群 Wire 化 = 把已落地的 1:1 Wire 每设备 leaf 骨架，对接到群已有的链上成员/`expected_epoch` 定序骨架**，从而
**根除「只读设备不能发」整套 primary/PIN/handoff 机器**、获得多端并发与按设备 PCS，且**链改为零**（成员表按账户、
设备 Welcome 走链下 relay、定序复用链上 `commit`；空-delta 推进 commit 已由 `same_account_empty_delta_commit_rekey_is_allowed` 验证放行）。
**链侧前置已清除**，剩余工作全在客户端 + relay；**核心权衡是群内设备数暴露**（终态由路线 B 修复）。
先做 G0 wasm spike 的 S1–S3，全绿即可立项。

> 以下 §15–§20 为**开发级补充**（接口契约 / 错误边界 / feature flag 与迁移 / 权重基准 / 可量化验收 / WBS），
> 把本文从设计评审稿升级为**可直接开工的开发文档**。命名与现有 1:1 Wire 代码（`nexchat/src/mls/`）对齐，
> 落地时以源码为准。

---

## 15. 接口契约（客户端）

> 目标：让 G1–G5 各 PR 有明确的类型边界。**复用** 1:1 Wire 既有接口，**新增**仅群侧差异（定序走链）。

### 15.1 群引擎 staged-commit 接口（复用，无新增方法）

群引擎复用 `OpenMlsEngine` 既有的**按 convKey** staged 方法（`g:{id}` 与 `d:…` 同签名，源码已实现）：

```ts
// nexchat/src/mls/openMlsEngine.ts（现状，群侧直接复用，convKey = `g:${groupId}`）
addMembersStagedByConv(convKey: string, keyPackages: Uint8Array[]): { commit: Uint8Array; welcome: Uint8Array };
removeMembersStagedByConv(convKey: string, memberIdentities: string[]): { commit: Uint8Array; welcome: Uint8Array };
selfUpdateStagedByConv(convKey: string): Uint8Array;
mergePendingByConv(convKey: string): void;
clearPendingByConv(convKey: string): void;
memberIdentities(convKey: string): string[];   // 设备名册（披露 UI）
hasGroup(convKey: string): boolean;
epochByConv(convKey: string): number;
```

> ✅ 关键复用点：staged 原语**已按 convKey 泛化**，群 `g:` 无需新增 wasm 方法。差别仅在「merge 时机」——
> 群在**上链 `commit` 成功后**才 `mergePendingByConv`（见 §15.3），而非 relay CAS 后。

### 15.2 群设备加入控制消息（复用 1:1 `device_join_*`，新增 `scope`）

复用 `directCommitCoordination.ts` 的三段式，**新增可选 `scope` 字段**区分 1:1 与群目标（向后兼容，缺省 = 1:1）：

```ts
// 扩展（加性、可选字段；旧消费者忽略 scope 即回落 1:1 行为）
interface DeviceJoinOfferControlMsg {
  t: "device_join_offer";
  convId: string;            // s:<account>
  device_id: string;         // 目标新设备
  conv_ids: string[];        // 1:1：d:… ；群：g:…（由 scope 区分）
  scope?: "dm" | "group";    // 新增：缺省 "dm"
}
interface DeviceJoinKpControlMsg {
  t: "device_join_kp";
  convId: string;
  device_id: string;
  kps: Array<{ conv_id: string; kp: string; scope?: "dm" | "group" }>;  // 每条带 scope
}
```

> CD 对 `scope==="group"` 的条目走 §15.3 群执行器；对 `"dm"` 走既有 1:1 执行器。**同一新设备一次握手可同时补齐群与 1:1**。

### 15.3 群设备执行器 `createGroupDeviceExecutor`（新增，仿 `createAddDeviceExecutor`）

与 1:1 的 `WireCommitExecutor` 同形，差别在 `runIntent` 产出后**经链上 `commit` 定序**，`commitAccepted` 改为「上链成功」回调：

```ts
// nexchat/src/mls/groupDeviceCommitExecutor.ts（新增）
export interface GroupDeviceExecutorDeps {
  engine: WireExecutorEngine;                 // 复用 §15.1 子集
  chain: ChainClient;                          // 群定序后端（替代 relay CAS）
  relay: Pick<RelayClient, "sendControl">;     // 仅用于 s:<account> 投递 Welcome/KP
  selfAddress: string;
}

// 语义（与 1:1 executor 对齐，但定序走链）：
// runIntent(intent): staged add_device/remove_device → { commitB64, welcomeB64, preEpoch }
//   preEpoch = engine.epochByConv(`g:${groupId}`)  →  作链上 commit 的 expected_epoch
// submit(intent): 上链 commit(groupId, preEpoch, commit, tree_hash, transcript_hash, group_info_cid,
//                              welcomes=[], member_delta={added:[],removed:[]})
//   ├─ Ok            → mergePendingByConv(`g:${groupId}`) + relay 投递 Welcome/KP 到 s:<account>
//   └─ EpochStale    → clearPendingByConv + syncGroupEpoch(追平) + 重新 runIntent（重试，上限见 §16）
```

> 与 1:1 的唯一结构差异：**1:1 用 relay `commit_slot` CAS 裁决 merge；群用链上 `expected_epoch` 裁决 merge**。
> staged→上链→merge 的「先链后并」次序杜绝「本地已推进、链上落败」分叉。

### 15.4 待退役接口（群侧）

| 接口 / 文件 | 动作 |
|---|---|
| `groupHandoffRuntime.ts`（群发送权运行时） | G6 删除群侧实例化；1:1 不涉及 |
| `sendingAuthority.ts`（群 HandoffReceipt） | G6 群侧不再调用 |
| `signingPinBackup` / `SigningPinBackupPanel` / `SigningPinRestoreButton`（群路径） | G6 删除群入口 |
| `appStore.groupSendMode` 三态 + `SendingAuthorityBanner` | G6 删除 |
| `engine.exportEscrowState`/`importEscrowVault`（群路径） | G6 群侧停用；1:1 与 EISA 不动 |

---

## 16. 错误处理与边界

| 场景 | 行为 | 落地点 |
|---|---|---|
| 上链 `commit` 返回 `EpochStale` | `clearPendingByConv` → `syncGroupEpoch` 追平 → 重 `runIntent`；**重试上限 5 次**，超限标记该群「设备加入待重试」并退避（指数 1s→16s） | §15.3 executor |
| 上链成功但 relay 投递 Welcome/KP 失败 | 链上 epoch 已推进**不可回滚**；新设备暂解不出 → 进入「待 Welcome」态，CD 按 §16 退避**重投 Welcome**（Welcome 由 staged 产出已缓存，幂等）；新设备亦可主动 `device_join_request` 重发 | CD + 新设备 |
| 新设备 `processWelcome` 失败（损坏/过期） | 丢弃该 Welcome，新设备重发 `device_join_request`；不影响群其它成员（其 epoch 已推进） | 新设备 |
| 成员侧 `verifyIncomingCommit` E2EI 校验失败 | `discardIncomingCommit`，**不** `processCommit`；记审计日志；该设备 leaf 不被接纳 | `followCommitGuard` |
| CD 选举抖动（双 CD 各发一条群 commit） | 链上 `expected_epoch` 只接受其一，落败方 `EpochStale` 自动退让；**不分叉** | 链 + executor |
| 加设备撞 别人加成员（同 epoch） | 同上，链上仲裁；设备加入方落败则追平后重提（empty-delta 可无条件重放） | 链 |
| 限频 `note_mls_action` 触发 | CD 合批（§8.2）；超限时延迟 Add 到下个窗口，UI 不报错（后台静默重试） | CD + `wireGroupJoinPlan` |
| 群被 `GroupFrozen`（治理冻结） | 链上 `commit` 拒绝；设备加入挂起，UI 提示「群已冻结」，解冻后自动恢复 | executor |
| 被移除设备仍尝试发 | 其 leaf 已不在树 → 本地 `encrypt` 失败；UI 提示「本设备已被移除，请重新加入」 | 引擎 |

---

## 17. Feature flag、迁移与灰度回退

### 17.1 开关

复用现有 flag 体系（`nexchat/src/config.ts`）：

```ts
// 复用：1:1 Wire 已有
wireMultileafEnabled: (import.meta.env.VITE_WIRE_MULTILEAF_ENABLED ?? "false") === "true",
// 新增：群 Wire 化独立开关（默认 OFF，与 1:1 解耦灰度）
wireGroupMultileafEnabled:
  (import.meta.env.VITE_WIRE_GROUP_MULTILEAF_ENABLED ?? "false") === "true",
```

- **默认 OFF**：关闭时群侧行为**零变化**（仍走轨 A 或单设备路径），保证可随时回退。
- **依赖**：`wireGroupMultileafEnabled` 隐含要求群引擎持本设备 signer（G1）；未达 G1 时即使置 true 也应 no-op + 警告。
- **双 Wire 同开**（`wireMultileafEnabled` + `wireGroupMultileafEnabled`）：`appStore` 通过 `createUnifiedWireAccountCoordinator`（`accountWireCommitCoordinator.ts`）共享**一个** `DirectAccountCommitCoordinator`——单次 CD 选举、单条 `s:<account>` presence、一次 `device_join_request` / 合并 offer（`d:` + `g:`）、`commit_intent` 按 conv 前缀路由（`d:` → relay CAS executor，`g:` → 链驱动）、KP/offer 扇出至 `DirectWireSession` + `GroupWireSession` join bridge。单 flag 时仍各自独立 CD。

### 17.2 迁移路径（轨 A 群 → Wire 群）

| 阶段 | 存量设备状态 | 迁移动作 |
|---|---|---|
| flag OFF（现状） | 轨 A：primary + 只读副设备 | 无变化 |
| flag ON、未迁移群 | 群仍是轨 A 单共享 leaf | 首次该群活跃时，由 primary（持 signer）发一次 `selfUpdate` 把自身转为「devA leaf」，其余设备走 §6.3 加设备 |
| flag ON、已迁移群 | 每设备独立 leaf | 正常 Wire 流程 |

> **迁移不可逆**：一旦群转为多 leaf，回退 flag 只影响**新群**；已迁移群保持 Wire（轨 A 运行时已删，无法回退单 leaf）。故灰度应**按账户**而非按群，且灰度名单一旦纳入不轻易移除。

### 17.3 灰度顺序

1. 内部账户（dev）→ 2. 小比例真实账户（监控 §18 指标）→ 3. 全量。每级 stop-loss：§18 指标超阈值即冻结放量。

### 17.4 relay 前置

群设备 Welcome/KP 走 `s:<account>`（已落地、休眠）。放量前确认生产 relay（Rust `relay-rs`）已部署该通道（参见总纲 §10.3）。

---

## 18. 权重 / 基准 / 性能预算

| 项 | 现状 / 预算 | 动作 |
|---|---|---|
| empty-`member_delta` commit 权重 | `commit` 权重按 `O(added)+O(removed)` 计；空 delta = 基础权重 | 复用既有 `WeightInfo::commit(0,0)`；**确认 benchmark 覆盖 (0,0) 档**，缺则补一条 |
| MLS 树规模 | Σ(账户×设备)，commit/解密随之增长 | 预算：单群有效 leaf ≤ **1500**（500 账户×3 设备）；超限告警 |
| 加设备 fan-out | O(活跃群数) 次 commit | 延迟 Add（§8.1）压到活跃群；CD 合批（§8.2） |
| `note_mls_action` 限频 | 每账户窗口 `MaxMlsActionsPerWindow` | 实测「换新机一次性补齐 K 个活跃群」是否撞限频；必要时调参或分批跨窗口 |
| vault 退役收益 | 删除群 vault 上传 churn（§原轨 A §3.5 C3） | Wire 化后群侧无 vault 上传，带宽下降 |

**监控埋点（放量前必备）**：① 单设备加群 commit 失败率 / `EpochStale` 重试次数；② 设备加入端到端时延（request→可发）；③ 单群 leaf 数分布；④ Welcome 重投次数。

---

## 19. 可量化验收阈值

| 维度 | 通过阈值 |
|---|---|
| **正确性** | S1–S3 wasm 全绿；多端并发群发 0 例 nonce 重用；被移除设备解密新消息成功率 = 0% |
| **加设备时延** | 兄弟在线时 request→可发 **P50 ≤ 5s、P95 ≤ 15s**（活跃群） |
| **定序** | 并发 commit 冲突 100% 由链上 `expected_epoch` 仲裁；落败方追平后重提成功率 ≥ 99%，0 分叉 |
| **死锁回归** | 「无 primary 在线即无人能发」场景**不可复现**（任一登录设备恒可发） |
| **fan-out** | 换新机补齐**仅活跃群**；休眠群 0 commit（延迟 Add 生效） |
| **限频** | 正常使用（≤ N 设备 × M 活跃群）不触发 `note_mls_action` 拒绝 |
| **隐私披露** | 群信息页显示「N 台设备 / ✓ 已验证」；移除设备 confirm 含 PCS 文案 |
| **回退** | flag OFF 时群侧行为与现状逐字节一致（快照对比） |

---

## 20. 工作分解（WBS）与文件清单

| 阶段 | 新增 / 改动文件 | 测试 |
|---|---|---|
| **G0** | `nexchat/mls-wasm/tests/group_wire_spike.rs`（仿 `hybrid_spike.rs`） | S1–S3 native |
| **G1** | `openMlsEngine`/`mls-wasm` 群引擎持 signer + `deviceLeafIdentity` 群侧；E2EI 嵌群 KP | 群多 leaf 往返 |
| **G2** | `groupDeviceCommitExecutor.ts`（新）；`directCommitCoordination.ts` 加 `scope`；`directWireSession` 群分支 | executor 单测 + verifyIncomingCommit |
| **G3** | `directAccountCommitCoordinator` 群意图路由；定序接 `chainClient.commit` + `syncGroupEpoch` | 链上定序回归（含既有 `same_account_empty_delta_commit_rekey_is_allowed`） |
| **G4** | ✅ `wireGroupJoinPlan.ts`（`planWireGroupJoin`）+ `GroupWireSession.activateGroup`/`deferredGroups`/`isGroupActive`；relay-only KP 已就位（last-resort 池/GC 留 G5/G6） | ✅ 延迟 Add 计划单测（4）+ 会话 lazy-add 端到端 |
| **G5** | ✅ `GroupWireSession.requestGroupPeerAdd` / `onPeerAddReq`（群版 peer-add，仿 SERIALIZATION_SPEC §3.8）+ `onPeerAddTimeout`/`peerAddFallbackMs` deps + `isGroupMember` fail-closed 鉴权；External Commit 仍可选未落地 | ✅ peer-add happy + 跨账户注入负向测（非成员 / 伪造绑定，3 测） |
| **G6** | ✅ `computeWireGroupRoster`/`WireGroupRoster`（`wireDeviceRoster.ts`）+ `appStore.wireGroupRosterFor`/`groupWireDeviceId` + `WireGroupDeviceSheet` + `ChatWindow` 群披露接线；轨 A 群侧 flag 门控退役（`ChatWindow`/`SendingAuthorityBanner`/`NewGroupChat`）。群移除按钮 + 实时会话接线留 G7 | ✅ `wireDeviceRoster.test.ts`（1:1 + 群名册，4 测）；全套件 464 绿、tsc 干净 |
| **G7** | ✅ 客户端验收矩阵 `groupWireAcceptance.test.ts`（会话层 WASM 端到端：多端并发群发 / 换机续发 / 移除设备 PCS 自愈 / 并发 `EpochStale` 仲裁 / 无主委派+CD 重选）；✅ relay 路由前置（`protocol.rs`：`device_join_*` 走 `s:`、群 Welcome 按 `toAddr` 走 `g:`、群 `peer_add_req` 盖章广播，+2 单测 / 15 测绿）；✅ `appStore` 实时会话接线 + `removeGroupWireDevice` + 群移除按钮点亮（flag 门控）；✅ 双 Wire 同开时统一 CD（`accountWireCommitCoordinator.ts` + `appStore` 接线 + 4 单测）；剩真实节点多端联调（节点依赖） | ✅ 4 场景绿；relay-rs 15 测绿；nexchat 全套件 + `accountWireCommitCoordinator.test.ts` 绿、tsc/lint 干净 |

> 依赖序：G0 →（G1 → G2 → G3）核心链路 → G4/G5 健壮性 → G6 退役 → G7 验收。G4/G5 可与 G3 并行。
