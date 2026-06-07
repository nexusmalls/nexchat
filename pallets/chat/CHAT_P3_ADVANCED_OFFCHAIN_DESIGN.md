# Chat P3 进阶能力 · 链下方案与边界设计

> 状态：设计 / 边界决策（**链上不新增 extrinsic**）
> 适用范围：`pallets/chat/{core,group,permission,common}` 及其链下投递层
> 关联：`CHAT_MODULES_CONSOLIDATION_DESIGN.md`、`core/src/lib.rs` §13 收敛说明、
> `CHAT_LARGE_FILE_SPEC.md`（大文件加密分块 / manifest / Pin 计费，文件类消息的 `body` 细化）

## 0. 一句话结论

P3 审计列出的「引用回复 / @提及 / reaction / 转发 / 阅后即焚 / 音视频信令」在本架构里**几乎全部属于链下职责**。
本文档把它们定义为**链下功能规格 + 明确的链上不承载边界**，而不是新增链上 extrinsic。

## 1. 为什么是链下：架构前提

本仓库已经在做「人类聊天消息迁出链」的收敛，P3 必须建立在这个前提上，而不是与之冲突：

- **core 私聊**：`send_message`（`Text/Image/File/Voice`）已在代码中**显式标注弃用**
  （见 `core/src/lib.rs` 的 Deprecation 注释，引《chat-core × MLS 收敛》§13）：人类消息
  改走**链下 MLS + 节点广播**，链上仅保留 `send_message`（仅 `System` 类，低频系统通道）。
- **group 群聊**：消息**全程链下**（MLS / RFC 9420），链上只存群元数据、握手日志与
  可选的 `anchor_message_digest` 审计锚点，**密文不触链**。
- **统一会话视图**：README「链上 / 链下边界」已写明群 `unread` / `last_active` 链上无从得知。

因此，把 reply / reaction / mention / forward / 阅后即焚 做成链上 extrinsic，会：
1. 往**正在被收窄/移除**的热路径堆存储，违背 §13；
2. 把交互元数据（谁回复谁、谁给谁点了什么 reaction）**明文上链**，违背端到端加密与隐私目标；
3. 给每个交互动作引入 gas 与区块膨胀，体验与成本都不可接受。

## 2. 链上 / 链下边界总表

| P3 能力 | 归属层 | 链上是否参与 | 说明 |
|---|---|---|---|
| 引用回复 reply | MLS payload | 否 | payload 内带被引用消息引用，密文内解析 |
| @提及 mention | MLS payload + 客户端索引 | 否 | 群聊才有意义；成员引用在密文内，客户端建本地未读/提及索引 |
| reaction | MLS payload + 客户端聚合 | 否 | 表情反应是高频轻量交互，天然链下；客户端做计数聚合 |
| 转发 forward | MLS payload | 否 | 携带来源引用 + 内容副本，由发送方客户端组装 |
| 阅后即焚 ephemeral | MLS payload + relay TTL + 客户端 | 否 | TTL 在 payload 声明，relay 到期不再投递，客户端到期本地销毁 |
| 音视频信令 | 链下信令通道（WebRTC/SFU） | 否 | 实时信令绝不上链；审计原文即「若产品需要」 |
| **审计存证（可选）** | group 既有锚点 | 既有，不新增 | 确需留痕时用现有 `anchor_message_digest` 锚 hash，不锚明文 |

## 3. MLS Payload 信封约定（链下统一格式）

所有 P3 交互通过**同一个可版本化的密文信封**承载，在 MLS application message 的明文 payload
里（即解密后）解析。链上对这些字段**完全无感**。

建议信封（解密后的逻辑结构，序列化用 CBOR/JSON 均可，客户端与 relay 约定其一）：

```jsonc
{
  "v": 1,                       // 信封版本
  "id": "<client-msg-id>",      // 客户端生成的消息唯一 id（去重 / 引用用）
  "type": "text|image|...",     // 内容类型
  "body": { /* 内容或 IPFS CID */ },

  // ---- P3 可选交互字段（均可缺省）----
  "reply_to":   "<msg-id>",                 // 引用回复：被引用消息 id
  "forward":    { "from_msg": "<msg-id>",   // 转发：来源消息引用
                  "from_conv": "<conv-id>" },
  "mentions":   ["<member-ref>", "..."],    // @提及：成员引用列表（群内）
  "reaction":   { "target": "<msg-id>",     // reaction：目标消息 + 表情 + 增删
                  "emoji": "👍", "op": "add|remove" },
  "ephemeral":  { "ttl_ms": 60000,          // 阅后即焚：相对 TTL
                  "burn_on": "read|deliver" }
}
```

要点：
- **向后兼容**：所有 P3 字段可选，老客户端忽略未知字段即可。
- **`reaction` 是独立消息**：reaction 作为一条普通 MLS 消息发送，`type` 可为 `reaction`，
  目标用 `reaction.target` 指向被反应的消息 id；接收端做聚合显示，不需要单独信道。
- **id 由客户端生成**：链上消息 id（core 的 `u64`）只对系统消息有意义；人类消息走链下，
  统一用客户端 id 做引用，避免依赖链上序号。

## 4. 各能力链下设计

### 4.1 引用回复 / 转发（reply / forward）
- 发送方客户端在信封写入 `reply_to` 或 `forward`，整体经 MLS 加密后投递。
- 接收端解密后据引用 id 在本地会话渲染引用块 / 转发卡片。
- 链上零参与；不存在「被引用消息一定在链上」的约束（人类消息本就不在链上）。

### 4.2 @提及（mention）
- 仅群聊有意义（1:1 私聊只有两人，提及无语义）。
- 成员引用放 `mentions`，密文内传递；客户端解密后据此建立「提及我」的本地索引与未读。
- 不上链：链上不应知道「谁在群里 @ 了谁」。

### 4.3 reaction
- 作为一条轻量 MLS 消息（`type=reaction`）发送，客户端按 `target` 聚合计数与去重
  （同人同表情 `add` 幂等，`remove` 撤销）。
- relay 与普通消息同等中继，无需特殊信道。
- 不上链：高频、轻量、强隐私，链上承载无收益。

### 4.4 阅后即焚（ephemeral / burn-after-read）
- 发送方在 `ephemeral` 声明 `ttl_ms` 与 `burn_on`（读后 / 送达后起算）。
- **三层执行**：
  1. **客户端**：到期从本地存储 + UI 销毁（核心执行点）；
  2. **relay**：对标记 ephemeral 的消息设置投递期限，过期不再向离线端补投；
  3. **MLS**：常规密钥轮换（commit/epoch 推进）保证前向保密，已销毁内容无法被新成员回看。
- 不上链：销毁是客户端/relay 行为，链上既无明文也无需记录销毁事件。

### 4.5 音视频通话信令
- 通过**独立链下信令通道**（WebRTC offer/answer/ICE，或 SFU/MCU 服务）完成。
- 可复用现有节点中继做信令转发；媒体流 P2P 或经 SFU，绝不经链。
- 链上参与：无。审计原文即「**若产品需要**」——属可选产品能力，非链能力缺口。

## 5. 链上明确「不做」清单（及理由）

| 不做的事 | 理由 |
|---|---|
| 新增 `reply_message` / `forward_message` extrinsic | 人类消息已迁出链（§13），不在弃用热路径加新入口 |
| 链上 reaction 存储 / extrinsic | 高频明文交互上链，破坏隐私 + 区块膨胀 |
| 链上 @提及 / 提及索引 | 群消息全链下，链上无群消息可供引用 |
| 链上阅后即焚状态机 / 定时销毁 hook | 销毁是端侧行为；链上记录销毁反而留痕、违背初衷 |
| 链上音视频信令 | 实时信令上链不可行 |

## 6. 可选的未来链上挂钩（默认不实现，仅记录）

仅当产品后续明确需要、且收益大于隐私/成本代价时再评估，**当前一律不实现**：

- **系统消息 reaction（niche）**：`send_message`（System）产生的消息确实在链上，理论上可对
  其加轻量 on-chain reaction；但场景狭窄，默认不做。
- **争议存证**：若仲裁/合规需要为某条链下消息留不可抵赖证据，复用 group 既有
  `anchor_message_digest`（锚 hash，不锚明文），无需新原语。
- **举报对接**：P2 已实现的 `permission::report` 以 IPFS CID 为对象/证据，已能覆盖
  「举报某条链下消息」的链上留痕，无需为 P3 单独扩展。

## 7. 对客户端 / relay 团队的交付边界

- **链上侧（本仓库）**：P3 不引入新 extrinsic / 新 storage；维持 §13 收敛方向。本文件即链上侧
  对 P3 的正式立场与边界。
- **链下侧（客户端 + relay）**：按第 3、4 节信封约定与执行分层实现 reply / mention / reaction /
  forward / ephemeral；音视频按第 4.5 节走独立信令通道。
- 如链下实现过程中发现确需链上原语，回到第 6 节流程单独评审，不在 P3 默认范围内开口子。

> 阅后即焚（§4.4）只是设备本地生命周期的一个子能力。完整的**设备端保留与清理策略**
> （按时间/条数/容量保留、热冷分层、LRU 淘汰、恢复路径与安全交互）见
> `CHAT_DEVICE_RETENTION_DESIGN.md`。
