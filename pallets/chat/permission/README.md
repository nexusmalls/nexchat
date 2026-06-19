# Pallet Chat Permission

聊天权限系统 pallet。已在 runtime 注册为 `ChatPermission`（pallet index **69**）。

基于**场景授权**的多上下文权限控制 + 隐私级别 + 链下能力令牌撤销锚（`CapabilityEpoch`）+
平台合规（治理禁言 / 举报）。人类私聊的门控与联系人关系**已链下化**；链上不再维护好友图谱或
黑/白名单明文。

## 1. 定位与边界

| 层 | 职责 |
| --- | --- |
| **链上** | 隐私级别、`CapabilityEpoch`、场景授权、平台禁言、举报存证 |
| **链下** | 联系人图谱、「允许向我私聊」能力令牌（接收方签名；relay 验证） |
| **拉黑/放行** | `bump_capability_epoch`（账户级作废）+ `inbox::revoke_tag`（每联系人定向） |

**与 inbox epoch 正交：** `CapabilityEpoch` 键为 `AccountId`；inbox 撤销纪元键为 `inbox_id`——
relay 验证盲化投递令牌时不需要接收方主账户。

**与 `core` 关系：** `ChatPermissionChecker::can_send_message` 门控 `do_send` 纵深防御；
生产 `send_message` 仅接受 System 且绕过权限闸门。人类私聊不经此 extrinsic。

## 2. 隐私收敛（审计 P1 / C 方案）

已**删除**（call_index 留空不复用）：

- 链上 `block_list` / `whitelist` 及 `block_user` / `unblock_user` / `add_to_whitelist` /
  `remove_from_whitelist`
- 链上好友图谱：`Friendships`、好友申请、备注/分组及对应 extrinsic / API

`Whitelist` 级别**语义等同** `FriendsOnly`（SCALE 索引保留）；真正联系人闸门在链下能力令牌。

## 3. Extrinsic 一览

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 0 | `set_permission_level(level)` | `Open` / `FriendsOnly` / `Whitelist` / `Closed` |
| 1 | `set_rejected_scene_types(types)` | 按场景类型拒绝授权聊天（最多 10 种） |
| 8 | `bump_capability_epoch()` | 递增本账户 `CapabilityEpoch`；作废此前签发的能力令牌 |
| 13 | `force_mute_account(who, until?)` | 治理平台禁言（发送方视角） |
| 14 | `force_unmute_account(who)` | 治理解除禁言 |
| 15 | `report(target, reason_cid)` | 举报存证（理由为 IPFS CID，链上无明文） |
| 16 | `resolve_report(id, upheld)` | 治理关闭并移除举报 |

留空索引：2/3（原拉黑）、4/5/8/9/10/11/12（原好友握手）、6/7（原白名单）——**不复用**。

场景授权的授予/撤销**无用户 extrinsic**，由业务 pallet 经 `SceneAuthorizationManager` trait 调用。

## 4. 权限检查优先级（`check_permission`）

1. **平台禁言** → `DeniedSenderMuted`（发送方被治理禁言）
2. **场景授权**（未过期 + 未被 `rejected_scene_types` 拒绝）→ `AllowedByScene`
3. **隐私级别**：
   - `Open` → `Allowed`
   - `FriendsOnly` / `Whitelist` → `DeniedRequiresFriend`（联系人由链下令牌判定）
   - `Closed` → `DeniedClosed`（**无场景授权时**）

**审计 U2：** 场景授权**有意覆盖** `Closed`——活跃交易上下文（订单/争议/做市）下对方必须能
就业务联系接收方；接收方可经 `rejected_scene_types` 按场景拒绝。

> 门控唯一事实来源：`check_permission` / `ChatPermissionChecker::can_send_message`。
> `has_any_valid_scene_authorization` **不**套用禁言/拒绝场景/隐私级别，仅诊断用。

## 5. 场景类型（`SceneType`）

| 变体 | 生产用途 |
| --- | --- |
| `MarketMaker` | 做市业务 |
| `Order` | 订单买卖双方（`pallet-entity-order` 接线） |
| `Memorial` | 纪念馆 |
| `Group` | 群成员↔群主可选 1:1（`GroupChatAuthorizer` 钩子） |
| `Custom` | 扩展场景 |
| `Direct` | **遗留 / 仅测试**；生产无授予；1:1 走链下能力令牌 |

## 6. 业务接线（runtime）

| 钩子 / 适配器 | 行为 |
| --- | --- |
| `GroupChatAuthorizer` | 群成员加入/离开 → `SceneType::Group` + `scene_id = group_id`（成员↔群主，O(1)） |
| `OrderChatAuthorizer` | 非即时订单创建/终态 → `SceneType::Order` + `scene_id = order_id`（尽力而为，吞错） |
| `GroupPlatformMuteCheck` | 群 MLS 写入路径读 `is_account_muted` |

业务 pallet 授予示例：

```rust
T::ChatPermission::grant_bidirectional_scene_authorization(
    *b"entorder",
    &buyer,
    &seller,
    SceneType::Order,
    SceneId::Numeric(order_id),
    Some(duration_blocks),
    Vec::new(), // metadata：生产调用方传空
)?;
```

## 7. 平台合规

- **平台禁言**：`MutedAccounts`；`is_account_muted` 供群 pallet / RPC；发送方在 `check_permission` 最高优先级拒绝。
- **举报**：`Reports` + `OpenReportCount`；举报人 `ReportCooldown`（runtime：1 分钟）；
  全局 `MaxOpenReports`（runtime：10_000）；治理 `resolve_report` 清理。

## 8. 存储

| 存储 | 说明 |
| --- | --- |
| `PrivacySettingsOf` | 权限级别 + `rejected_scene_types` |
| `CapabilityEpoch` | 账户 → u32 撤销纪元 |
| `SceneAuthorizations` | 排序用户对 → 场景授权列表（**明文配对**，固有权衡 P2） |
| `MutedAccounts` | 平台禁言状态 |
| `Reports` / `NextReportId` / `OpenReportCount` / `LastReportAt` | 举报 |

**P2 隐私权衡：** `SceneAuthorizations` 暴露业务上下文配对（订单/群等来源记录本已公开）；
`metadata` **明文上链**——调用方必须传空或不透明引用（生产均传空）。

## 9. 配置（`Config`）— runtime 当前值

| 项 | runtime 值 |
| --- | --- |
| `MaxScenesPerPair` | 64 |
| `GovernanceOrigin` | Root 或技术委员会多数 |
| `MaxReportCidLen` | 128 |
| `MaxOpenReports` | 10_000 |
| `ReportCooldown` | 1 分钟 |

## 10. Runtime API 与 RPC

`runtime_api::ChatPermissionApi`（只读、免费）：

| 方法 | 说明 |
| --- | --- |
| `check_chat_permission(sender, receiver)` | `PermissionResult` |
| `get_active_scenes(user1, user2)` | 场景列表（含过期标记） |
| `capability_epoch(who)` | 能力撤销纪元 |
| `is_account_muted(who)` | 是否平台禁言 |
| `get_privacy_settings_summary(user)` | 隐私摘要 |

Node JSON-RPC：`chat_checkPermission`、`chat_getActiveScenes`、`chat_capabilityEpoch`、
`chat_isAccountMuted`、`chat_privacySummary`（`node/src/chat_rpc.rs`）。

## 11. Trait 端口

| Trait | 用途 |
| --- | --- |
| `SceneAuthorizationManager` | 业务 pallet 授予/撤销/延期场景授权 |
| `ChatPermissionChecker` | `can_send_message` — `core` 等消费方门控 |

## 12. 依赖关系

```
pallet-chat-common  ←── pallet-chat-permission（bump_u32_epoch + min_blocks_elapsed）
```

被 `pallet-chat-core`（`ChatPermission` 端口）、`pallet-chat-group`（runtime 禁言检查）、
`pallet-entity-order`（订单场景）消费；不依赖 `common` 以外的 chat crate。

## 13. 迁移

`migration.rs`：v0 → v1 重写 `PrivacySettingsOf`（去除 block/whitelist 字段），分批清理
遗留好友图谱存储；`on_runtime_upgrade` 钩子调用，含幂等单测。

## 14. 权重与基准

6 个用户/治理 extrinsic 有 `WeightInfo` + `benchmarking.rs`；已加入 runtime 基准清单。
主网前应在参考硬件重跑 `runtime-benchmarks`。

## 15. 上线审计摘要（2026-06-19）

| 维度 | 结论 |
| --- | --- |
| **隐私 P1** | ✅ 黑/白名单/好友图谱已下链；`CapabilityEpoch` + inbox tag 撤销 |
| **权限门控** | ✅ 平台禁言 → 场景 → 隐私级别；U2 场景覆盖 Closed 已文档化 |
| **死枚举清理** | ✅ `DeniedBlocked` / `DeniedNotInWhitelist` 已删 |
| **平台合规** | ✅ 禁言 + 举报冷却/上限 |
| **业务接线** | ✅ 群成员↔群主、订单买卖双方场景授权 |
| **存储迁移** | ✅ v0→v1 + 遗留清理 + 幂等测试 |
| **Runtime 接线** | ✅ index 69 + API + node RPC |
| **单测** | ✅ 30 项 + 2 项迁移测试通过 |
| **缺口（非阻塞）** | ⚪ 场景配对明文为固有权衡（P2）；⚪ 联系人判定依赖链下 relay；⚪ 主网前重跑 benchmark；⚪ mock `CHARLIE` dead_code warning |

**总评：达到上线标准。** 隐私收敛（P1/C 方案）、权限门控、平台合规与关键业务接线均已落地；
链下能力令牌与 relay 为配套链下组件，不构成本 pallet 阻塞项。
