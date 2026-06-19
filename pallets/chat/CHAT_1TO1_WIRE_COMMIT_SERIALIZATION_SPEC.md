# 1:1 Wire 多 leaf · 账户内协调设备选举 + relay Commit 串行化 · 子规范

> 状态：设计草案 · v1（待评审）；**闸二 CAS 已在 Rust relay-rs 落地并测试；闸一（CD 选举 + 意图路由 + 生命周期 + 真实 `add_device`/`rekey` staged 执行器 + 会话编排/presence）已落地并测试**（见 §3.6）
> 定位：[`CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md`](./CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md) §5 / WIREIFY §6.3–§8.4 / 开放项 O2、O3 的展开规范（1:1 链下定序；群 Wire 化改用链上 `expected_epoch`，见 WIREIFY §7）。
> 范围：**仅 1:1（链下 pairwise MLS）** 的 Commit 定序。群聊有链上 `expected_epoch` 全序，**不适用本规范**。
> 关联：`nexchat/src/mls/directConv.ts`（owner = 较小地址）、`directHandshake.ts`（add/Welcome/recover）、
> `directMlsRegistry.ts`（控制面汇聚）、`nexchat/relay-rs`（唯一 relay 实现，`echoSelf`/`s:`/`msgId`）。

---

## 0. TL;DR

CN：1:1 是链下 pairwise MLS，**没有链上 epoch 全序**。Wire 多 leaf 把「我账户多设备」放进同一个 pairwise
群，于是**任意成员都能发 Commit**（加我新设备、rekey、移除设备），多个 Commit 指向同一 epoch 时会**分叉**。
本规范用两道闸消除分叉：①**账户内协调设备**——我账户同一时刻只有一台设备被选为「可发 Commit」，其余设备的
Commit 意图改为**请求协调设备代发**；②**relay 按 `(conv, epoch)` 串行化**——relay 对同一会话同一 epoch
只接受**第一条** Commit，其余返回 `EpochStale`，落败方重取状态后重试。应用消息（普通聊天）**不受限**，并发自由。

EN: 1:1 is off-chain pairwise MLS with **no on-chain epoch total order**. Wire multi-leaf puts my multiple
devices into one pairwise group, so **any member may commit** (add my new device, rekey, remove a device);
concurrent commits onto the same epoch fork. This spec adds two gates: (1) an **intra-account coordinator
device** — only one of my devices is "commit-eligible" at a time, others route their commit intent to it;
(2) **relay serialization per `(conv, epoch)`** — the relay accepts only the FIRST commit at a given epoch
and returns `EpochStale` to the rest, which re-fetch and retry. Application messages are unconstrained.

---

## 1. 问题陈述

### 1.1 为什么 1:1 会分叉而群不会
- **群**：`pallet-chat-group::commit(expected_epoch)` 由链全序仲裁，落败者得 `EpochStale`（主文档 §5.4）。
- **1:1**：不建链上群（隐私不变量），**无链上仲裁**。relay 只做投递，默认不理解 MLS 语义。

### 1.2 Wire 多 leaf 新增的并发源
| 并发源 | 触发 | 是否需要 Commit |
|---|---|---|
| 普通聊天消息 | 用户发文本/媒体 | 否（application message，不进 epoch） |
| 加我新设备 | 换机 / 新增设备（§4.3） | **是**（Add + Commit） |
| 移除我设备 | 设备丢失 / PCS（§8） | **是**（Remove + Commit） |
| 我方设备 rekey | 定期 PCS / 自愈 | **是**（self-update Commit） |
| 对端任意成员上述操作 | 对端多设备 | **是** |

→ **只有 Commit 需要串行化**；application message 因每 leaf 独立 ratchet 天然并发安全（已由 `hybrid_spike.rs` C4 验证）。

### 1.3 目标
- 同一 pairwise 会话、同一 epoch，全网最多 **1 条 Commit 被采纳**。
- 落败方**可恢复**（重取状态、重试或放弃），不死锁、不丢应用消息。
- **不引入链上改动**（1:1 仍全链下）。
- 对端为单设备、非 Wire 用户时**完全透明**（不要求对端实现本规范）。

---

## 2. 闸一：账户内协调设备选举（client 侧）

> 目的：把「我账户 N 台设备各自发 Commit」收敛为「**至多一台**代表我账户发 Commit」，从源头减少冲突。
> 这是 §1.2 中**我方**那几行的去重；对端冲突由闸二兜底。

### 2.1 协调设备的定义
- 每个账户在每个时刻至多一个 **coordinator device**（CD）。
- 仅 CD 可以发起**我方触发的** Commit（加设备 / 移除 / rekey）。
- 非 CD 设备产生 Commit 意图时，**不直接发 Commit**，而是经 relay 控制面发 `commit_intent` 给 CD（§2.4）。

### 2.2 选举规则（确定性，免协商优先）
默认用**确定性 + 在线性**两级规则，避免复杂共识：

1. **候选集** = 我账户当前**在线**（relay 有活跃 WS）的设备集合，按 `DeviceId`（签名公钥指纹）字典序。
2. **CD = 候选集中字典序最小者**。确定性、各设备本地可算（与 `directHandshakeOwner` 同思路）。
3. **无人在线**：无 CD；我方 Commit 意图挂起，直到有设备上线（或由对端代发，§4.3 情形二）。

> 选「字典序最小」而非「在线最久」：前者**无需全局时钟/协商**，各设备凭「在线设备名单」即可一致推出同一 CD。
> 在线名单来自 relay 的账户内 presence（§2.3）。

### 2.3 在线名单来源（presence）
- 复用 relay 账户内扇出通道 `s:<account>`（已落地）：设备上线/下线时广播 `presence{device_id, online}`。
- 每台设备维护本账户的 `online_devices` 集合；据此本地算 CD。
- **presence 抖动容忍**：presence 变更后加 `CD_SETTLE_MS`(建议 2s) 静默期再切换 CD，避免频繁选举。

### 2.4 非 CD 设备的 Commit 意图路由
```
非 CD 设备 D 想加自己/移除某设备/rekey：
  1. D 经 s:<account> 发 commit_intent { kind, payload, req_id } 给 CD
  2. CD 校验意图合法（E2EI：发起者确属本账户）后，代发对应 Commit（闸二串行化）
  3. CD 经 s:<account> 回 commit_result { req_id, ok | epoch_stale }
  4. D 收到 ok 后从 relay backlog / echo 追平到新 epoch
```
- 「新设备加入自己」是特例：新设备**还不在群里**，无法自己发 Commit，本来就**必须**由 CD（已在群的设备）代发（§4.3 情形一）。
- CD 离线/易主：意图带 `req_id` 幂等；新 CD 接管后可重放未完成意图（去重靠 `req_id`）。

### 2.5 退化与边界
- **单设备账户**：候选集恒为自己 → 自己即 CD，规则退化为现状，零额外开销。
- **CD 与对端同时 Commit**：闸一无法跨账户协调（对端是另一账户）→ 由**闸二**兜底。
- **脑裂**（两设备各自认为自己在线名单不同）：可能短暂选出两个 CD → 仍由闸二保证「同 epoch 仅一条被采纳」，脑裂只退化为「多一次重试」，不破坏一致性。

---

## 3. 闸二：relay 按 `(conv, epoch)` 串行化（relay 侧，权威）

> 目的：跨账户（我 vs 对端）与脑裂残留冲突的**权威仲裁**。这是正确性的最终保证，**不能仅依赖闸一**。

### 3.1 relay 维护的状态
对每个 pairwise 会话 `conv`（= `d:{sorted_a}:{sorted_b}` 的不透明路由键）：
```
CommitSlot[conv] = {
  epoch:        u64,    // 当前已采纳到的 epoch（= 最近被接受 Commit 后的 epoch）
  committed_at: u64,    // 最近采纳时间（ms）
  last_commit_msg_id:  // 幂等：同 msg_id 重投视为已采纳
}
```
- relay **不解析 MLS 密文**；`epoch` 由发送方在信封头**明文携带**（`commit_epoch` 字段，仅整数，不泄露内容）。
- 该 `epoch` 是「**这条 Commit 的前置 epoch**」（即 `expected_epoch`，与群的 `commit(expected_epoch)` 同义）。

### 3.2 接受规则（CAS 语义）
```
收到 Commit 帧 { conv, commit_epoch, msg_id, ciphertext }：
  if msg_id == CommitSlot[conv].last_commit_msg_id:        # 幂等重投
      return ACCEPTED (idempotent)
  if commit_epoch == CommitSlot[conv].epoch:               # 指向当前 epoch → 第一个赢
      CommitSlot[conv].epoch += 1
      CommitSlot[conv].last_commit_msg_id = msg_id
      fan-out 该 Commit 给 conv 双方所有设备（含 s:<account> 账户内扇出）
      return ACCEPTED
  else:                                                     # commit_epoch < 当前 → 落败
      return EPOCH_STALE { current_epoch: CommitSlot[conv].epoch }
```
- **唯一胜者**：同 `epoch` 的并发 Commit，先到 relay 者 `epoch += 1`，后到者 `commit_epoch` 已 < 当前 → `EPOCH_STALE`。
- application message 帧**不带** `commit_epoch` / 不走本槽位，relay 原样扇出（并发不受限）。

### 3.3 落败方处理
```
设备收到 EPOCH_STALE { current_epoch }：
  1. 从 relay backlog 拉取 [本地 epoch .. current_epoch] 的 Commit，processCommit 追平
  2. 重新评估意图是否仍需要（如"加新设备"在追平后发现已被另一路径加入 → 放弃）
  3. 若仍需要：以新的 expected_epoch 重发 Commit（回到 §3.2）
  4. 重试上限 MAX_COMMIT_RETRY（建议 5）后放弃并上报 UI（落入 §5 兜底）
```

### 3.4 留存与可用性
- Commit 帧按 `(conv, epoch)` 留存至**双方至少各一台设备 ack**（复用 `echoSelf` + ack 语义）。
- backlog 留存窗口 `RETAIN_MAX` **≥ 普通应用消息**（Commit 是追平的权威源，对应主文档 §13.5）。
- 超 `RETAIN_MAX` 未追平 → 物理边界，落入重握手（§5）。

### 3.5 relay 不变量（隐私）
- relay 仅见 `conv` 路由键（化名）、`commit_epoch`（整数）、`msg_id`、不透明密文。
- **不得**学习 `conv` 与具体账户对的关联（沿用 `pallets/chat/inbox/` 的 sealed/inbox 边界）。
- `commit_epoch` 明文是可接受泄漏（仅单调计数，无内容）；如需更严，可改为对 relay 不可见的"序号承诺"，但增复杂度，v1 不做。

### 3.6 实现状态（已落地）

> 注：relay 唯一实现为 Rust `nexchat/relay-rs`。闸二已落地
> 并有单测。CommitSlot 为**内存态**，不触快照/journal，故不破 relay-rs 的磁盘 parity 红线
> （`core/tests/js_compat.rs`）。

**闸二（relay CAS）——relay-rs 实现：**
- `nexchat/relay-rs/core/src/commit_slot.rs`：纯函数 `try_accept_commit(slots, conv, commit_epoch, msg_id)
  -> CommitDecision{Accepted{next_epoch} | Idempotent | EpochStale{current_epoch}}`，实现 §3.2 规则
  （含重启重新播种、幂等、并发同 epoch 落败）。7 条单测全绿。
- `d:` commit 分支**仅当帧携带明文 `commit_epoch` 时启用** CAS；落败回
  `commit_reject{reason:"epoch_stale", convId, current_epoch, msgId}` 且**不存储、不扇出**；无 `commit_epoch`
  的旧 2-leaf 握手 commit 原样透传（严格加性，逐字节兼容既有 1:1）。CommitSlot 在 server 内存态、**不序列化到磁盘**
  （`relay-rs` 为 `Inner.commit_slots`）。

**槽位跨重启重建（已落地）：** CommitSlot 虽不写盘，但**启动时由已持久化的 MLS commit backlog 重建**——
`reseed_slots_from_commits`（core），逐会话取已存胜出 Commit 的**最大** `commit_epoch`
播种 `epoch = max+1` 并记录其 `msgId`。**只有胜者会被存储**（落败者不存储不扇出），故重建后槽位精确等于重启前。
这关闭了原「重启后由首条所见 Commit 重新播种」窗口——否则一条**陈旧**落败 Commit 可能在客户端自愈前被二次采纳
（短暂分叉）；重建后陈旧 Commit 落 `epoch_stale`、且重投的那条胜出 Commit 仍判**幂等**。**不新增任何磁盘格式**，
故 relay-rs 磁盘 parity 红线（`core/tests/js_compat.rs`）不受影响。单测见 `commit_slot.rs`。接线：
`Inner::from_persist → reseed_commit_slots`。

**闸一（账户内协调设备选举 + 意图路由）——客户端骨架已落地：**
- `nexchat/src/mls/directCommitCoordination.ts`：**纯**逻辑 + 编解码——CD 确定性选举（`pickCoordinatorDevice` =
  在线集字典序最小）、presence 静默期选举状态机（`applyPresenceUpdate` / `tickCoordinatorElection`，`CD_SETTLE_MS`）、
  `s:<account>` 自通道 codec（`presence` / `commit_intent` / `commit_result`）、Wire commit 构造（`buildWireDmCommit`
  注入 `commit_epoch` + `msgId`）、`commit_reject` 解析与重试判定（`MAX_COMMIT_RETRY`）。13 条单测全绿。
- `nexchat/src/mls/directWireCommitLifecycle.ts`：**纯** CD 侧 Commit 生命周期 reducer（§4.1/§3.3）——
  `awaiting →(settle_timeout 隐式采纳)→ delivered` / `→(epoch_stale)→ catch_up_and_retry →(caught_up)→
  resend_commit`，超 `MAX_COMMIT_RETRY` 则 `reply_give_up`。6 条单测全绿。
- `nexchat/src/mls/directAccountCommitCoordinator.ts`：运行时——把 presence/意图路由接到 live `RelayClient`，
  并以 reducer + 隐式采纳静默计时器（`settleMs`）驱动 CD 侧生命周期：收到 `commit_intent` → 经注入的
  `WireCommitExecutor.runIntent` 跑 OpenMLS（捕获操作前 epoch）→ 发 `commit_epoch` Wire Commit → 静默窗口内无
  `commit_reject` 即投递 Welcome + 回 `commit_result{ok}`；落败则 `catchUpAndRerun` 追平后用新 epoch+msgId 重发。
  MLS 执行经 `WireCommitExecutor` 接口注入。3 条驱动集成测试（含 epoch_stale 追平重发）全绿。
- `nexchat/src/relay/relayClient.ts` + `wsRelay.ts`：`ControlMsg` 增 `presence`/`commit_intent`/`commit_result` 与
  commit 的 `commit_epoch`/`msgId` 字段；WS 传输新增 `onCommitReject`。

**staged-commit 原语（wasm，关键正确性修复）：**
- `nexchat/mls-wasm/src/lib.rs`：新增 `addMembersStaged` / `selfUpdateStaged`（跑操作但**不** `merge_pending_commit`）、
  `mergePending`（ACCEPT 后合并）、`clearPending`（EPOCH_STALE/放弃时丢弃）。旧 `addMembers`/`removeMembers` 保持
  auto-merge（群轨 A 透传，向后兼容）。`OpenMlsEngine` 暴露对应 `*StagedByConv` / `mergePendingByConv` / `clearPendingByConv`。
- **为何关键**：旧路径 `addMembers` 立即合并——落败时本地已强制合并出分叉 epoch 且 1:1 无链上 commit 日志可回退。
  staged 后，落败只需 `clearPending`（永远安全，群回到操作前 epoch 的 operational 态），**杜绝永久分叉**。

**`add_device` / `rekey` / `remove_device` 执行器（真实 OpenMLS，staged 语义）：**
- `nexchat/src/mls/directWireCommitExecutor.ts`：`createAddDeviceExecutor` 返回真实 `WireCommitExecutor`——
  `runIntent` 把 `add_device`（`addMembersStaged([新设备KP])`）/ `rekey`（`selfUpdateStaged`）/ `remove_device`
  （`removeMembersStaged([target])`）跑成 **staged** commit、捕获**操作前** epoch 作 wire `commit_epoch`；
  `commitAccepted`→`mergePending`、`commitAbandoned`/`catchUpAndRerun`→`clearPending`；`deliverWelcome` 经 relay 把
  Welcome 投给我的账户（rekey/无 Welcome 时跳过）。
- **设备区分凭证（解锁 `remove_device`）**：`directConv.ts` 加 `deviceLeafIdentity(account, deviceId)=`{account}#{deviceId}`` +
  `accountFromLeafIdentity`（剥 `#` 还原账户）。1:1 Wire 引擎用设备区分 leaf identity 后，`removeMembers([{account}#{dev}])`
  可定位**单个**设备 leaf（wasm `resolve_leaf_indices` 已支持 `hint` 精确 / `hint#...` 前缀匹配）→ 按设备 PCS。
  > 注：**真实链路**（`appStore` `useChainCp`）目前每账户一个 `MlsClient`、identity=纯 SS58，群+1:1 共用。设备区分凭证须
  > 落在**独立的 1:1 引擎**（设计图 `MlsClient(1:1,本设备leaf)`）——把 1:1 Wire 接到独立引擎是剩余接线项，**不动**群轨 A 引擎。
- `directWireCommitExecutor.test.ts`：**WASM 实跑**（6 条全绿）——① 同账户两设备 + 对端单设备 staged-add+merge 三方收敛互解密
  （对应 `hybrid_spike.rs` C2–C5）；② staged-add 被 `commitAbandoned` 丢弃后群**无分叉**仍可收发；③ rekey staged→merge→对端跟进；
  ④ 落败方持 staged 仍 `processCommit` 胜出 Commit（OpenMLS merge 自动清 stale pending）→追平→重 staged→四 leaf 收敛；
  ⑤ `remove_device` 按 `{account}#{dev}` 定位单设备 leaf + 按设备 PCS（被移设备读不到新 epoch）；⑥ 非本人会话 / 缺 KP 拒绝。
- `catchUpAndRerun`：**先 `clearPending`（永远安全，防分叉）**，再读 epoch；若已追平（≥ toEpoch）则重新 staged 并以新
  epoch 重发，否则抛「未追平」。协调器对其做**有界轮询**（`catchupPollMs` × `maxCatchupPolls`，默认 400ms×8）等待胜出
  Commit 经 `DirectMlsRegistry` 正常控制面应用到**同一引擎**——OpenMLS `merge_staged_commit` 会**自动清掉**落败方仍持有的
  staged pending（无需先手动清），故落败方可直接 `processCommit` 胜出 Commit 追平。预算耗尽才 give-up → 回退
  `recoverMemberSession` 重握手（§5）。
- **WASM 实跑验证并发恢复**（`directWireCommitExecutor.test.ts` 第 6 例）：alice/bob 跨账户同 epoch 各自 staged-add，CAS
  让 alice 胜；bob（持自己的 staged pending）`processCommit` alice 胜出 Commit → 自动清 pending + 追平 → 重新 staged-add +
  merge，双方四 leaf 收敛互解密。
- **执行器主动 backlog 拉取（已落地）**：落败方首次未追平时，`WireCommitExecutor.requestCatchUp(conv)` 经
  `RelayClient.requestMlsBacklog(account, conv)` 请求 relay **按需重投**该会话已存的胜出 Commit（relay 侧 `mls_backlog_req`
  鉴权后从 MLS 邮箱筛 `convId` 重投给请求者本人），使追平**确定性成功**，不再仅依赖 registry/偶发扇出的时序；协调器在
  **首次**未追平时触发一次（`attempt===0`），随后有界轮询。WS 传输生效；mock（BroadcastChannel 全扇出）空操作。测试：
  `relay-rs/server` 单测（`mls_backlog_req` 按需重投 + 非本人请求被 `auth_reject`）、`directAccountCommitCoordinator.test.ts`（首次未追平
  仅触发一次 `requestCatchUp`）、`directWireCommitExecutor.test.ts`（`requestCatchUp` 经 relay 拉自身 backlog）。

**闸一接入会话生命周期 + presence 广播：**
- `nexchat/src/mls/directWireSession.ts`：`DirectWireSession` 编排器——构造真实 executor + `DirectAccountCommitCoordinator`，
  `start()` 订阅控制面并广播 presence 上线、`stop()`/`onRelayDisconnected()` 广播下线、`addDevice`/`rekey` 提交意图。
  `submitIntent` 修复：自身为 CD 时**本地执行**该意图（此前仅执行被委托意图，CD 自身意图被漏执行）。
- `directWireSession.test.ts`：3 条全绿——start/stop presence 上下线；单设备 CD 本地 `add_device` 端到端（intent→staged
  commit→settle→merge→ok）；`rekey` 走同一本地执行路径。
- `directWireSessionJoin.test.ts`（**H6 移除设备自愈 E2E**）：**会话层** `DirectWireSession.removeDevice` WASM 端到端——三 leaf 群
  （bob 持有 + alice 两设备）中，alice 唯一在线设备被选为 CD，经 intent→串行化 staged remove→settle→merge 移除老设备，对端跟随到
  新 epoch；断言存活设备 ↔ 对端在新 epoch 仍可互解密，**被移除设备（停在旧 epoch）解不出新 epoch 消息**（按设备 PCS）。驱动的是
  H5 `removeWireDevice` UI 动作所走的同一路径。

**生产接线（flag-gated，已落地）：**
- `config.ts` 加 `wireMultileafEnabled`（`VITE_WIRE_MULTILEAF_ENABLED`，**代码默认 false；生产 `.env.production` 已设 `=true`**，下次生产构建/部署生效；上线前置：relay 须支持 `peer_add_req`/commit-slot CAS/`mls_backlog_req`，既有 1:1 会话在全新 `wire:{account}` 引擎上重握手）。`appStore`：开启后在链路里
  实例化**独立** wire 引擎 `new OpenMlsEngine()` + `init(deviceLeafIdentity(self, device), persistKey="wire:{account}")`
  （绝不覆盖账户/群快照）；`DirectMlsRegistry` 路由到 wire 引擎且 **`chain: undefined`**（仅经 relay 交换 KeyPackage——
  两端 KP 均出自各自 wire 引擎，**不与链上账户 KP 跨引擎失配**，代价：wire 路径无离线对端链上 KP 引导）；新增
  `realEngineFor`/`isOpenMlsEngine`——`d:` 会话的 encrypt/decrypt/`engineFor` 路由到 wire 引擎，群/`g:` 仍走账户引擎；
  `DirectWireSession` 在 wire 引擎上 `start()`（presence 广播 + CD 选举 + 意图路由），relay 重连重广播。
- **关闭（默认）时零行为变化**：wireEngine=null，所有路由回退账户引擎，既有 1:1 不受影响（全量测试绿）。

**待做**：E2EI 更深硬化（创建者自身 leaf 绑定〔受 OpenMLS 读取接口所限〕，见 §3.9 剩余）。
（已落地：多设备加入触发 §3.7、对端代 Add §3.8、执行器主动 backlog 拉取 §3.6、**槽位跨重启重建**（启动时由持久化
commit backlog 重建闸二槽位，关闭重启重新播种窗口、parity 安全、§3.6/§9）、无兄弟 join 隐私收敛 + 恢复门控 §3.8、
E2EI 设备 leaf 凭证**MLS 内凭证 + 成员侧复验**（请求级一阶段 `cred` 已退役）§3.9，且 **§3.7 / §3.8 两条 relay-only 加入路径经统一 `kpInMlsBinding`
三态闸、两条 `d:` 跟随链经 `verifyIncomingCommit` 成员侧复验**；**archive 中间空窗补齐的最终一致性**——账户级历史正文由解锁时
`K_archive` 协调恢复覆盖，嫁接进 1:1 时（`onGraftConvs`）再触发 `scheduleGapRefill` 有界延迟重拉 archive 幂等合并空窗正文，
见混合设计 §4.5 与 `msgArchiveSync.test.ts`。）

### 3.7 多设备加入触发（join trigger，协议已落地，auto-wire 待 registry 抑制）

> 目的：让账户的**新设备**自动被嫁接进该账户**所有已有 1:1**，无需对端参与（同账户内 CD 代办 Add）。

**三段式协议**（全部经账户自通道 `s:<account>`，仅同账户设备可见；编解码在 `directCommitCoordination.ts`，`ControlMsg`
联合已扩展，`parseAccountSelfControl` 全量校验）：
1. `device_join_request{ device_id }` — 新设备广播；**仅当选 CD** 响应（且忽略自身 echo）。
2. `device_join_offer{ device_id, conv_ids[] }` — CD → 新设备：列出本引擎持有的 `d:` 会话（`listJoinableConvs`）；**仅被
   点名设备**响应。
3. `device_join_kp{ device_id, kps:[{conv_id, kp}] }` — 新设备：对**仍缺失**（`!hasGroup`）的会话各造**一次性** KeyPackage
   交回；CD 据此对每个会话 `submitIntent(add_device)` → 走 §2/§3 串行化 → 隐式采纳合并 → `deliverWelcome` 扇出账户。

**新设备收 Welcome**：`DirectWireSession` 自带 graft-Welcome 接收器，消费 `pendingGrafts` 中会话的 Welcome 并
`processWelcomeByConv` 真正加入——**补上 owner 侧缺口**（按会话 `DirectMlsCoordinator` 仅在非握手 owner 时处理 Welcome）。

**幂等**：新设备对已持有的会话不再造 KP（offer 阶段过滤），故重复 `announceJoin` 不产生新 Commit / 不重复加 leaf。

**auto-wire（已落地，graft-only 所有权）**：每个 `d:` 会话由**唯一**机制拥有——本地发起的新会话走 `DirectMlsRegistry`
1:1 握手；兄弟已有的会话走嫁接，registry **完全退场**。
- **registry 退场**：`DirectMlsRegistry.markGraftManaged(conv)` 后，registry 对该会话**不发起握手、不应用控制面、不
  recover、`onRelayConnected` 跳过**，`isReady` 仅看本地是否已有嫁接群 → 杜绝与嫁接竞争**分叉**多 leaf 群。默认（未标记）
  路径零变化。
- **session 端到端拥有**嫁接会话：消费加入 Welcome 后，**跟随该会话后续每条 Commit**（rekey/add/remove）保持 epoch 同步
  （registry 不再代劳）。
- **appStore 接线**：wire 路径下，新设备启动即 `announceJoin()` 并**推迟** roster/联系人 1:1 握手，直到 join 阶段安定：
  - 收到 offer → `onGraftConvs` 标记这些会话 graft-managed（从 registry 摘除）；
  - `onJoinSettled`（offer 到达 **或** 无 CD 的 `joinSettleMs` 回退超时）**仅触发一次** → 对 roster 其余（非嫁接）会话发起
    常规握手。首设备（无兄弟 CD）经回退超时以零嫁接安定，正常握手全部联系人。

**测试**：`directWireSessionJoin.test.ts`（双 session + 共享总线 + 真实 `OpenMlsEngine`）验证 announce→offer→kp→add 全链路、
三 leaf 收敛、互解密、幂等、**offer→graft-managed + 安定一次 + 嫁接设备自动跟随后续 rekey Commit**、**首设备回退超时安定**；
`directMlsRegistry.test.ts` 验证 graft-managed 会话不发起握手/忽略入站/`isReady` 取本地群/`recoverPeer` no-op，且默认路径不变。

**剩余**：无。archive 历史拼接已闭环——执行器主动 backlog 拉取（§3.6）补可解密窗口；嫁接进会话后由
`onGraftConvs → scheduleGapRefill` 有界延迟重拉 `K_archive` 幂等合并**不可 MLS 解密**的中间空窗正文（混合设计 §4.5）。

### 3.8 对端代 Add（peer-assisted Add，已落地）

> 目的：当新设备的**全部兄弟设备都不在线**（§3.7 的兄弟嫁接路径走不通）时，让**对端**（1:1 的另一方）把新设备的
> leaf 接进已有群——既补齐多设备，又**绝不分叉**多 leaf 群。

**三方角色**：请求方 = 新设备所属账户 A 的某设备；对端 = 会话另一方账户 B（在线、持有该 1:1 群）；加入设备 = A 的新设备。

**协议（单帧请求，跨账户走 pairwise `d:` 通道）**：
- `peer_add_req{ convId(d:A:B), requester_account=A, device_id, kp }`：A 的新设备造**一次性** KeyPackage，把会话登记为
  待嫁接（`pendingGrafts`，以消费随后的 Welcome），发往会话**另一方**。
- 对端 B 收到后执行**校验三连**，通过则走 B 自己的**串行化** `add_device`（闸一 CD + 闸二 CAS，与兄弟 add 同一执行器路径），
  唯一差别：`CommitIntentPayload.welcomeTo = A` → 执行器把 Welcome 投到**请求方账户 A**（而非自身），relay 扇出到 A 的所有设备，
  新设备消费之、其余（在线/离线经邮箱）跟随同一 Commit。

**安全模型（无 E2EI 时的信任锚）**：
- 跨账户加人若不设防，伪造的 `peer_add_req` 可**注入窃听 leaf**。当前以 **relay 账户认证**为锚：
  - relay 对 `peer_add_req` **盖章**认证发送者 `_senderAccount = ws._nexAccount`，并**丢弃未认证**会话的该帧（无法盖章则无法分辨真伪）。
  - 对端校验三连：(a) `_senderAccount === requester_account`（防冒充——攻击者无法以 A 的身份通过 relay 认证）；
    (b) `requester_account` 是 `convId` 的**另一方**且非自身；(c) 自身**确实持有**该群。
- 结论：账户级认证已足以挡住**跨账户冒充注入**——攻击者无法以 A 认证，故无法让 B 为"伪 A"加 leaf；A 用错 KP 只伤自己。
  E2EI 可验证凭证链（验 KP leaf 归属 A）是**更强**硬化，列为后续。

**与 registry / §3.7 的合流（appStore 接线）**：
- join 安定（`onJoinSettled`）若**兄弟有应答**（offer 非空）→ 这些会话归嫁接、其余为全新 → 常规握手；**无需**对端代 Add。
- 若**无兄弟应答**（空）→ 对每个联系人先发 `peer_add_req`：已持有该 1:1 的对端会嫁接我们（其 Welcome → `markGrafted` →
  `onGraftConvs` 标记 graft-managed → registry **退场不分叉**）；在 `PEER_ASSIST_FALLBACK_MS`（默认 4s）内**未嫁接**的会话按
  **全新会话**处理 → registry 常规握手。由此修掉了"无兄弟时对已有群直接重握手会重置对端群"的潜在分叉。

**测试**：`directWireSessionJoin.test.ts` WASM 端到端——对端把请求方新设备嫁接进**已有**群（非分叉）、三 leaf（含离线老设备经
relay 跟随 Commit）收敛同 epoch、新设备↔对端互解密、`onGraftConvs` 回报、**盖章发送者≠请求方时拒绝**（无 Commit、epoch 不变）；
`directWireCommitExecutor.test.ts` 验 `welcomeTo` 把 Welcome 投到请求方账户且对方可消费入群；`relay-rs/server` 单测验
relay 把 `peer_add_req` 路由到另一方并盖章认证发送者、**未认证请求被丢弃**。

**隐私收敛（已落地）**：无兄弟 join 安定不再向**全部联系人**广播 `peer_add_req`。`planWireJoinTargets`（纯函数，
`wireJoinPlan.ts` + 单测）按权威的**已有 1:1 线索表**切分：已有 1:1 对端 → 定向 `peer_add_req`（仅向真实 1:1 对端，
不外泄"新设备上线"给非 1:1 联系人）；无 1:1 的联系人 → 常规 1:1 握手（建新会话，非"新设备"信号）；已嫁接会话两者皆排除。
- **恢复门控**：线索表对新设备**仅在云恢复安定后权威**（首设备的既有 1:1 来自 K_index/contacts 云恢复）。故 appStore 在
  无兄弟分支**等待** `awaitRestoreSettled`（`offchainSync.phase` 终态 / `WIRE_JOIN_RESTORE_WAIT_MS=8s` 上限 / 同步关闭则
  本地线索已同步可知），再规划。**附带修掉**旧的潜在分叉：registry 现在**只对确认无 1:1 的联系人**握手，绝不对线索尚未
  加载的已有群发起会重置对端的新握手。兄弟在线路径（offer）不受影响、仍秒级。

**剩余**：见 §3.9——relay-trustless 账户绑定（MLS 内凭证 + 成员侧复验）均已落地、请求级一阶段 `cred` 已退役；仅余更深硬化（可选）。

### 3.9 E2EI 设备 leaf 凭证（device-leaf credential，MLS 内凭证 + 成员侧复验已落地；请求级一阶段 `cred` 已退役）

> 目的：把 MLS leaf 身份（不透明字符串 `account#deviceFp`）**密码学绑定**到账户**链上 SS58 钥**，使对端在嫁接前可
> **不信任 relay**（relay-trustless）地验证「这个 KeyPackage 确属所声称的账户」，硬化对端代 Add（§3.8）。

**缺口**：leaf identity 仅是 `BasicCredential(account#deviceFp)`，与账户密钥**无绑定**——伪造一个 KeyPackage 即可声称任意账户。
§3.8 的 relay 盖章（`_senderAccount`）是**第一道**门，但它**信任 relay**；恶意/被攻陷 relay 可伪造盖章。

**历史（一阶段「绑定层」，已退役）**：最初请求方用账户 SS58 钥对**一次性 KeyPackage 字节**签名
（`ctx = "nexus/chat-wire/device-leaf-cred/v1"`），随 `peer_add_req.cred`（hex）发出供对端 relay-trustless 校验。该请求级 `cred`
是加性硬化的过渡形态；**在全引擎都嵌入下文 MLS 内凭证后已退役**——`cred` 字段、`signDeviceBinding` 接线、`deviceLeafBindingBytes`
/`verifyDeviceLeafBinding` 及其回退分支均已移除。现行机制为下文 MLS 内凭证。

**签名者接线**：`signRawWithAccountKey`（`chain/signer.ts`）——内置 keyring `pair.sign` 原样签名（可验）；注入器钱包返回 null
（外部扩展可能包裹 `<Bytes>…</Bytes>` 致不可验），`mock` 同样返回 null。无可验签名者的引擎不装绑定 → 其 KP 在跨账户 §3.8 路径
落 `absent` 被拒（见下三态闸）。

**MLS 内凭证（现行机制，已落地）**：绑定作为**自定义 leaf-node 扩展**（`ExtensionType::Unknown(0xF7E2)`）**驻留 MLS 内**，
随 leaf 走、超越 add 时刻持久。

- **绑定对象**：设备**稳定的 MLS leaf 签名钥**（区别于已退役一阶段的「一次性 KeyPackage 字节」）——账户 SS58 钥签
  `ctx2 ‖ account ‖ deviceId ‖ leafSigKey`（`ctx2 = "nexus/chat-wire/device-leaf-key-cred/v1"`）。
  该 leaf 钥对设备所有 KeyPackage 复用，故绑定**一次算出、对每个 KP 持续有效**。
- **mls-wasm 原语（rebuild 已落地）**：`signaturePublicKey()`（取稳定 leaf 钥）、`setLeafBinding(sig)`（装入，瞬态不入快照 →
  守逐字节红线）、`generateKeyPackage()` 装有绑定时把它作 leaf-node 扩展嵌入（并在 leaf capabilities 声明，否则 validate 拒）、
  `keyPackageBinding(kp) → { identity, signatureKey, binding }`（在 add 路径 validate 后提取，畸形 KP 即拒）。
- **接线**：appStore 在 wire 引擎 init 后用账户钥签稳定 leaf 钥并 `setLeafBinding` → **每个** wire KeyPackage 自带 MLS 内绑定。
- **统一闸（`kpInMlsBinding`，三态）**：两条 **relay-only** 加入路径共用一个校验：返回 `valid`（携带且由 `expectedAccount`
  的 SS58 钥对其稳定 leaf 钥签名）/ `invalid`（携带但伪造、账户不符、或 KP 畸形 → **必丢**）/ `absent`（未携带）。
  - **对端代 Add（§3.8）`onPeerAddReq`**：`expectedAccount = requester`（**对方**账户）；这是**跨账户**路径，**要求** `valid` 方嫁接——
    `invalid`/`absent` 一律丢弃（请求级 `cred` 回退已退役；缺绑定的外来 KP、含被攻陷 relay 注入的无绑定 KP，皆不嫁接）。
  - **多设备加入触发（§3.7）`onDeviceJoinKp`**：嫁接的是**兄弟**设备，故 `expectedAccount = 我方自身账户`；`invalid` 即跳过该 KP，
    `absent`（未嵌入绑定的旧引擎）在此同账户、`s:` 通道把关的路径上照旧放行。被攻陷 relay 往 `device_join_kp` 注入外来 KP 无法伪造我方账户签名 → 落 `invalid` 被丢。
- **归纳论证**：每条 add 路径在接纳前都从 KP 校验绑定 → 群内**每个 leaf 均已验证**，无需运行时遍历 ratchet tree
  （OpenMLS 0.8.1 亦**未**经 `MlsGroup` 暴露读取**其他成员** leaf-node 扩展的安全接口，故刻意不依赖事后回读，改由 add 时归纳保证）。
- **测试**：native `e2ei_credential_spike.rs`（自定义 leaf 扩展过 build/validate/add；并钉死**持久性**：在 leaf 创建处嵌入的绑定
  挺过 add-commit path-update 与 self-update（含默认参数）→ 故只需在 KeyPackage/建群时嵌入，commit/rekey 不必重附）+ `e2ei_binding_api.rs`
  （`MlsClient` 往返：装绑定→生成→提取，未绑定为空，清除生效）；TS `deviceLeafCredential.test.ts`（leaf-key 绑定真签验真 / 跨账户冒充拒 /
  换钥拒 / deviceId 换拒 / 坏签名不抛错）；`directWireSessionJoin.test.ts` WASM 端到端（§3.8 KP 自带绑定被验证并嫁接；§3.8 伪造绑定
  即不同账户钥签被拒、即使被攻陷 relay 伪造盖章仍被拒；**§3.7 join-trigger 外来 `device_join_kp` 绑定非我方账户钥签被拒**）。

**三阶段：成员侧复验（已落地）**：跟随者处理进入 Commit 时，**独立**确认该 Commit **新增**的每个 leaf 都账户绑定（绑定由所声称账户
SS58 钥签名）且该账户是会话两方之一——而非盲信**执行 add 的一方**。弥补跨账户跟随路径的残余缺口：恶意/有缺陷提交方混入外来或错标
leaf，会在跟随者**合并该 epoch 前**被 relay-trustless 拦下。

- **mls-wasm 原语（rebuild 已落地）**：`inspectCommitBindings(conv, commit)` 把进入 Commit 处理为 *staged*（**不合并**）、返回其新增
  每个 leaf 的 `{identity, signatureKey, binding}`，并按 conv 缓存 `(commit字节, StagedCommit)`；`processCommit` 见**同字节**缓存即复用
  该 staged commit 合并 → **消息只 `process_message` 一次**（避免二次处理推进 ratchet 致失败）；`discardIncomingCommit(conv)` 在绑定非法时
  丢弃暂存。`kpInMlsBinding` 的 leaf→绑定提取重构为共享 `kp_binding_of`。
- **纯模块 `followCommitGuard.ts`**：`verifyIncomingCommit(engine, conv, commit)` 三态语义——`true` 放行（已验证 / 引擎无检视 / Commit
  未加绑定 leaf）、`false` 拒绝（绑定验证失败或绑定到非会话方账户 → 已 `discardIncomingCommit`）。绑定为加性硬化，**缺失**绑定（旧版对端）放行。
- **接入两条 `d:` 跟随链**：跨账户对端 `DirectMlsCoordinator.applyCommit`（`directHandshake.ts`）与同账户嫁接 `DirectWireSession`
  的 graft-commit 跟随（经新增 `graftCommitQueue` 串行化，使复验 `await` 不打乱 epoch 顺序）。track-A `g:` 群不走此闸（无绑定）。
- **测试**：`directWireSessionJoin.test.ts` WASM 端到端——跟随者**接受**真实账户绑定的 Add commit（epoch+1）、**拒绝**伪造绑定者
  （丢弃、epoch 不变）。

**剩余（更深硬化，可选）**：
- **群创建者自身 leaf 的绑定**：技术上可经 `MlsGroup::builder().with_leaf_node_extensions(...)` 嵌入（spike 已验证可建+持久）。
  但 OpenMLS 0.8.1 **未**经 `MlsGroup` 暴露读取**他人** leaf 扩展的安全接口 → 创建者 leaf 的绑定**对端无法回读校验**，当前为
  *write-only* 故**未实现**（避免引入不可用的死代码）。已核实 0.8.1 公开面：`export_ratchet_tree()` 返回的 `RatchetTree(Vec<Option<Node>>)`
  内部节点字段私有且无公开遍历器，`full_leaves()` 仅实现在内部 `TreeSync` 上（不经 `MlsGroup`/`RatchetTree` 暴露），`members()` 只给
  `Member`（无 leaf 扩展）——除非裸解 TLS 线格式或等上游加 tree 访问器，否则无法安全回读。待对端可在**握手/Welcome 处**拿到创建者的 leaf 钥绑定
  （请求级交换或上游新增 tree 访问器）后再补，使 1:1 双向都被对端验证。

> **已退役**：请求级一阶段 `cred`——全引擎嵌入上文 MLS 内凭证后移除，跨账户 §3.8 改为**要求** MLS 内绑定（`valid`），
> 较旧的「`absent` 回退 `cred`」更强且无 relay 信任缺口。

---

## 4. 两闸协同：典型时序

### 4.1 我方加新设备（情形一，最常见）
```
新设备 P_new → s:<account> → CD：commit_intent{ kind: add_device, kp }
CD → relay：Commit(add P_new, commit_epoch=E)
relay：epoch E→E+1，ACCEPTED，扇出
CD → P_new：commit_result{ ok }；P_new 从 backlog 追到 E+1，processWelcome 入群
对端：processCommit 跟进 E+1（看到 alice 多一个 leaf）
```

### 4.2 我 Add 撞对端 rekey（跨账户冲突 → 闸二仲裁）
```
t0：CD 发 Commit(add, epoch=E)；对端发 Commit(rekey, epoch=E)
relay：先到者（设 CD）→ E+1 ACCEPTED；对端 → EPOCH_STALE{current=E+1}
对端：拉 backlog 追到 E+1 → 重评 rekey 仍需 → 重发 Commit(rekey, epoch=E+1) → ACCEPTED → E+2
最终：E+1=add、E+2=rekey，双方收敛，无分叉
```

### 4.3 我两设备脑裂双 CD（闸一残留 → 闸二兜底）
```
设备 D1、D2 各自误判为 CD，都发 Commit(epoch=E)
relay：先到者 ACCEPTED→E+1；另一方 EPOCH_STALE → 追平 → 多半发现意图已达成 → 放弃
结果：仅一条被采纳，脑裂只多一次重试
```

---

## 5. 全失败兜底
- 落败重试超 `MAX_COMMIT_RETRY` / backlog 超 `RETAIN_MAX` / 我全设备离线超窗：
  → 落入现有 `recoverMemberSession` / `recoverOwnerSession`（`directMlsRegistry.recoverPeer`）**重握手**。
- 重握手后历史正文仍由 `K_archive` archive 自愈（混合设计 §4.5），不丢可读历史。

---

## 6. 配置参数（建议默认）

| 参数 | 默认 | 含义 |
|---|---|---|
| `CD_SETTLE_MS` | 2000 | presence 变更后切换 CD 的静默期 |
| `MAX_COMMIT_RETRY` | 5 | 落败方 EPOCH_STALE 重试上限 |
| `RETAIN_MAX`（Commit） | ≥ 应用消息留存窗 | Commit backlog 留存窗口 |
| `COMMIT_INTENT_TTL_MS` | 30000 | 非 CD 设备 commit_intent 等待 CD 回执超时 |

---

## 7. 信封字段增量（relay 契约）

在既有传输信封（relay 组件的 sealed/inbox 投递契约 / 主文档 §13.3）上，Commit 帧新增：
```jsonc
{
  "conv": "0x..",          // pairwise 路由键（化名）
  "kind": "commit",        // 区别于 application / control
  "commit_epoch": 7,       // 明文整数：本 Commit 的前置 epoch（= expected_epoch）
  "msg_id": "0x..",        // 幂等键
  "ciphertext": "<opaque>" // MLS Commit 密文，relay 不解析
}
```
- 控制面 `commit_intent` / `commit_result` / `presence` 走 `s:<account>` 账户内扇出（已落地通道）。
- 严格加性：旧 relay 不认识 `kind=commit` 槽位时退化为普通扇出（**会失去串行化** → 故 relay 必须升级后才启用 1:1 Wire）。
- `peer_add_req`（§3.8，对端代 Add）：relay **盖章**认证发送者 `_senderAccount = ws._nexAccount`、路由到会话另一方、
  **丢弃未认证**会话的该帧。接收方校验 `_senderAccount === requester_account` 防跨账户注入 leaf。E2EI 账户归属（§3.9）
  **驻留 KeyPackage 的 leaf 节点内**（MLS 内凭证），接收方直接从 `kp` 做 relay-trustless 校验（第二道门，**要求** `valid`）；
  请求级一阶段 `cred` 字段已退役。
- `mls_backlog_req{ account, convId }`（§3.6，主动追平）：**仅认证账户本人**可拉取；relay 从该账户 MLS 邮箱筛 `convId`
  的已存控制（Commit/Welcome）**重投给请求者本人**（非全账户扇出）。非本人 → `auth_reject{op:"mls_backlog_req"}`。

---

## 8. 测试与验收

- **单设备透明**：对端单设备非 Wire 用户全程不受影响（无 commit_intent / 无 presence）。
- **并发应用消息**：不走串行槽位，多设备并发发不被阻塞（已由 `hybrid_spike.rs` C4 覆盖密码学侧）。
- **同 epoch 双 Commit（我方脑裂）**：仅一条 ACCEPTED，另一条 EPOCH_STALE → 追平 → 放弃/重试，最终单一 epoch 链。
- **跨账户冲突（我 Add 撞对端 rekey）**：两条 Commit 串成相邻 epoch，双方收敛。
- **CD 易主**：CD 下线 → 新 CD 接管未完成 `commit_intent`（`req_id` 幂等，不重复 Add）。
- **relay 幂等**：同 `msg_id` 重投不二次推进 epoch。
- **超窗兜底**：backlog 超 `RETAIN_MAX` → 触发重握手引导，历史正文仍可读。
- **relay 单测**：`(conv, epoch)` CAS 接受/拒绝、扇出、ack 留存——Node + `relay-rs` 双实现 parity（守逐字节红线）。
- **relay 按需追平**：`mls_backlog_req` 鉴权后按 `convId` 重投已存 Commit 给请求者本人；非本人 `auth_reject`。
- **relay 对端代 Add**：`peer_add_req` 盖章认证发送者并路由到另一方；未认证丢弃。
- **E2EI 设备 leaf 凭证（§3.9，MLS 内凭证 + 成员侧复验；请求级一阶段 `cred` 已退役）**：
  - MLS 内凭证：native `e2ei_credential_spike.rs`（自定义 leaf 扩展过 build/validate/add）+ `e2ei_binding_api.rs`
    （`MlsClient` 往返/空绑定/清除）；TS leaf-key 绑定真签验真 / 冒充拒 / 换钥拒 / deviceId 换拒 / 坏签名不抛错；端到端 KP 自带绑定
    被验证并嫁接、跨账户 §3.8 缺绑定（`absent`）拒、伪造绑定拒（含被攻陷 relay 伪造盖章仍拒）。
  - 成员侧复验：端到端跟随者**接受**真实账户绑定的 Add commit（epoch+1）、**拒绝**伪造绑定者（丢弃、epoch 不变）
    （`directWireSessionJoin.test.ts`）。

---

## 9. 开放项

| # | 开放项 | 备注 |
|---|---|---|
| S1 | `commit_epoch` 明文是否可接受 | v1 接受（仅计数）；高隐私档可改序号承诺，增复杂度 |
| S2 | CD 选举是否需更强活性（如租约） | v1 用确定性 + presence + 闸二兜底，足够；若 presence 不可靠再引入租约 |
| S3 | 对端也是 Wire 多设备时的双向 CD | 双方各自独立选 CD，互不感知；闸二天然处理跨账户 |
| S4 | 与群轨 A 的 relay 槽位是否复用 | 群有链上全序，**不**走本槽位；relay 按 `kind` 区分 |
| S5 | CommitSlot 跨重启 | **已落地**：启动时由持久化 MLS commit backlog 重建（`reseed_slots_from_commits` / `reseedSlotsFromMailbox`），关闭重启后重新播种窗口；**不新增磁盘格式**，parity 红线不动（§3.6 实现状态） |

---

## 10. 一句话结论

**1:1 Wire 的一致性 = 闸一（账户内协调设备，减少我方冲突）+ 闸二（relay 按 `(conv,epoch)` CAS 串行化，权威仲裁）。**
闸二是正确性底线、不可省；闸一是优化、可降级。两者都**不动链**，对端单设备完全透明。应用消息始终并发自由。
