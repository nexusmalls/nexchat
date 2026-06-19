# Pallet Chat Inbox

链下投递信箱注册表 pallet。已在 runtime 注册为 `ChatInbox`（pallet index **78**）。

盲化一次性投递令牌（基线 **RFC 9474 Blind RSA**）的最小链上锚点：注册不透明 `inbox_id`，
公布 relay **离线**验证令牌所需的 inbox 维度 `epoch` 与 `revoked_tags`。Blind-RSA 密钥、
消息内容、人际关系**均不上链**；relay / 盲签发实现属链下组件，不在本仓。

> ⚠️ 本文档随代码维护。v1 使用**签名 controller + 押金**反垃圾；为不可关联，controller
> **必须**是与主聊天账户无关的一次性密钥。账户无关（unsigned + inbox 钥签名）注册为后续加固项。

## 1. 定位与边界

| 层 | 职责 |
| --- | --- |
| **链上** | `inbox_id → { controller, epoch, revoked_tags, deposit }` 注册表 |
| **链下** | Blind RSA 盲签/兑付、per-inbox spent set、消息投递（relay） |
| **链不做** | RSA 运算、存储 IPK、存储消息、推断谁与谁通信 |

**`inbox_id` 绑定：** 客户端选定 32 字节不透明句柄，链下绑定 `inbox_id = H(IPK ‖ salt)`；
签发公钥由发送方携带，relay 自验证——链只认 `inbox_id` 字节。

## 2. 与 `CapabilityEpoch` 的关系（正交）

| | `pallet-chat-permission::CapabilityEpoch` | 本 pallet `epoch` |
| --- | --- | --- |
| **键** | `AccountId` | `inbox_id` |
| **用途** | 账户级 / 合规能力令牌撤销 | 盲化投递令牌新鲜度 |
| **relay 需账户？** | 是（比对签发者账户纪元） | **否**（仅 inbox 维度） |

两者**刻意不共用**，避免 relay 把 inbox 链回接收方主账户。

## 3. Extrinsic 一览

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 0 | `register_inbox(inbox_id)` | 注册信箱；预留 `InboxDeposit`；epoch 从 0 开始 |
| 1 | `bump_epoch(inbox_id)` | epoch +1 并**清空** `revoked_tags`；作废此前全部令牌 |
| 2 | `revoke_tag(inbox_id, tag)` | 定向撤销单个联系人标签（`ct_c`） |
| 3 | `deregister_inbox(inbox_id)` | 注销并退还押金；未知 inbox 的令牌不可投递 |
| 4 | `unrevoke_tag(inbox_id, tag)` | 解除误撤的单个 tag（**不**轮换 epoch） |
| 5 | `transfer_controller(inbox_id, new_controller)` | 控制权与押金迁移（controller 密钥轮换） |
| 6 | `force_deregister_inbox(inbox_id)` | 治理强制注销（controller 丢钥回收）；仅 `ForceOrigin` |

所有变更 extrinsic（除 force）要求调用者为该 inbox 的 **controller**。

### 撤销语义

- **`bump_epoch`**：整信箱作废——内嵌 epoch 不匹配的全部令牌失效；`revoked_tags` 清空。
- **`revoke_tag`**：仅拒绝携带该 `tag` 的令牌，不影响其他联系人。
- **`unrevoke_tag`**：恢复单个联系人，无需 bump（避免误伤全体）。

## 4. 类型

| 类型 | 说明 |
| --- | --- |
| `InboxId` | `[u8; 32]` 不透明信箱句柄 |
| `ContactTag` | `[u8; 32]` 每联系人撤销标签（设计文档 `ct_c`） |
| `InboxRecord` | `controller` / `epoch` / `revoked_tags` / `deposit` / `created_at` |

## 5. 存储

| 存储 | 说明 |
| --- | --- |
| `Inboxes` | `inbox_id → InboxRecord` |
| `InboxCountByController` | 每 controller 已注册信箱数（反囤积） |

## 6. 配置（`Config`）— runtime 当前值

| 项 | runtime 值 |
| --- | --- |
| `InboxDeposit` | 0.5 NEX（`UNIT / 2`） |
| `MaxRevokedTags` | 256（满则须 `bump_epoch` 清空） |
| `MaxInboxesPerController` | 16 |
| `ForceOrigin` | Root 或技术委员会多数 |

## 7. Runtime API 与 RPC

`runtime_api::ChatInboxApi`（只读、免费）：

| 方法 | 说明 |
| --- | --- |
| `inbox_epoch(inbox_id)` | 当前 epoch；未注册 `None` |
| `is_tag_revoked(inbox_id, tag)` | 定向撤销；未注册 `false` |
| `inbox_exists(inbox_id)` | 是否已注册 |

Node JSON-RPC：`chat_inboxEpoch`、`chat_isTagRevoked`、`chat_inboxExists`（`node/src/chat_rpc.rs`）。

Relay 校验伪码：令牌新鲜 ⟺ `token.epoch == inbox_epoch(inbox_id)` 且
`!is_tag_revoked(inbox_id, token.tag)` 且 `inbox_exists(inbox_id)`。

## 8. 事件

`InboxRegistered` / `InboxEpochBumped` / `ContactTagRevoked` / `ContactTagUnrevoked` /
`InboxControllerTransferred` / `InboxDeregistered` / `InboxForceDeregistered`

## 9. 错误

`InboxAlreadyExists` / `InboxNotFound` / `NotController` / `TooManyRevokedTags` /
`TagAlreadyRevoked` / `TagNotRevoked` / `TooManyInboxes`

## 10. 依赖关系

```
pallet-chat-common  ←── pallet-chat-inbox（bump_u32_epoch + reserve/unreserve_deposit）
```

不依赖 `permission` / `core` / `group`；与 `CapabilityEpoch` 正交。

## 11. 权重与基准

全 7 个 extrinsic 有 `WeightInfo` + `benchmarking.rs`；权重 dev 链实测（`src/weights.rs`）。
已加入 runtime 基准清单。主网前应在参考硬件重跑 `runtime-benchmarks`。

## 12. v1 隐私取舍（必读）

1. **controller 上链**：`inbox_id → controller_account` 在链上可见——controller 须为**一次性密钥**，
   与主聊天 `AccountId` 分离；relay 只读 inbox 维度状态。
2. **IPK 不上链**：不可关联性靠 `H(IPK)` 绑定在客户端/relay 侧完成。
3. **后续加固**：unsigned + inbox 钥签名注册（设计文档 §3 / §10），降低 controller 关联面。

## 13. 上线审计摘要（2026-06-19）

| 维度 | 结论 |
| --- | --- |
| **职责边界** | ✅ 最小锚点；无 RSA/消息/关系存储 |
| **epoch 正交** | ✅ inbox 键 ≠ `CapabilityEpoch` 账户键 |
| **撤销模型** | ✅ 整信箱 bump + 定向 tag + unrevoke 误撤恢复 |
| **反垃圾** | ✅ 押金 + controller 信箱上限 + 标签上限 |
| **治理回收** | ✅ `force_deregister_inbox` + 押金退还 |
| **controller 轮换** | ✅ `transfer_controller` 押金迁移 + 计数更新 |
| **Runtime 接线** | ✅ index 78 + `ChatInboxApi` + node RPC |
| **权重 / 基准** | ✅ 全 extrinsic benchmark + runtime 清单 |
| **单测** | ✅ 19 项通过（`cargo test -p pallet-chat-inbox`） |
| **缺口（非阻塞）** | ⚪ relay/盲签实现在链下（本仓外，设计既定）；⚪ v1 controller 关联面待后续 unsigned 加固；⚪ 主网前重跑 benchmark |

**总评：达到上线标准。** 链上锚点职责清晰、测试覆盖完整；relay 与 RFC 9474 实现属配套
链下组件，不构成本 pallet 的上线阻塞项。
