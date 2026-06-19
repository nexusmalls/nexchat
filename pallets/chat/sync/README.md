# Pallet Chat Sync

账户派生加密同步锚（**EISA**）pallet。已在 runtime 注册为 `ChatSync`（pallet index **79**）。

聊天同步/恢复设计的 **C 层**：以不透明 `anchor_id = blake2_256(anchor_pk)` 为键，存储客户端加密的
`SyncManifest`（conv-index / contacts-vault / msg-archive blob 的 CID 清单）。链**只存密文**，
不解密、不与 inbox 耦合。

多设备总纲见 [`CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md`](../../CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md)
（EISA 为换机/重装恢复路径之一）。

## 1. 定位与边界

| 层 | 职责 |
| --- | --- |
| **链上** | 每 `anchor_id` 一条加密清单 + `updated_at` LWW + 押金 + clear 墓碑 |
| **链下** | `vault_master` 派生 `anchor_pk`、AES-GCM 加解密 SyncManifest、IPFS/relay 拉 blob |
| **链不做** | 明文 CID、账户↔锚关联、inbox 投递、DH/AEAD |

**与 inbox 正交：** inbox 锚盲化投递令牌；EISA 锚加密同步清单——键控与用途均不同。

**助记词可重算：** `anchor_pk` 由客户端 `vault_master` 确定性派生，新设备零外部依赖即可定位锚
（优于 v1 `InboxId` 键控草案）。

## 2. 授权与付费分离

| 角色 | 职责 |
| --- | --- |
| **锚密钥（Ed25519）** | 对冻结 payload 签名——**真正授权** |
| **签名 origin（AccountId）** | 仅付手续费 +（首次 publish）反垃圾押金 |

后续可换 proxy / 一次性账户 / 赞助付费方，**无需**迁移 storage（`depositor` 仅在首次 publish 记录）。

## 3. Extrinsic 一览

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 0 | `publish_sync_anchor(anchor_pk, updated_at, ciphertext, anchor_sig)` | 发布/更新锚；`anchor_id` 由链计算 |
| 1 | `clear_sync_anchor(anchor_pk, anchor_sig)` | 删除锚；押金退给 `depositor`；写入墓碑 |
| 2 | `force_clear_sync_anchor(anchor_id)` | 治理强制清除（无锚签名）；逃生门 |

### 签名字节合同（冻结）

**Publish：** `PUBLISH_CONTEXT ‖ genesis_hash ‖ anchor_id ‖ updated_at(LE u64) ‖ blake2_256(ciphertext)`

- `PUBLISH_CONTEXT` = `nexus/chat-sync/publish/v1`

**Clear：** `CLEAR_CONTEXT ‖ genesis_hash ‖ anchor_id ‖ stored_updated_at(LE u64)`

- `CLEAR_CONTEXT` = `nexus/chat-sync/clear/v1`
- 绑定**当前**已存 `updated_at`，防 clear 签名跨状态重放

### 校验规则

| 规则 | 说明 |
| --- | --- |
| LWW | `updated_at >=` 已存值；`==` 允许幂等重发 |
| 幂等 no-op | 相同 `updated_at` + 相同密文 → 不写状态、**不**重置限频时钟 |
| 墓碑 | clear 后 publish 须 `updated_at > ClearedAt`（防链历史复活） |
| 时钟上界 | `updated_at <= now_ms + MaxClockSkew`（防自锁 `u64::MAX`） |
| 密文长度 | `>= MIN_CIPHERTEXT_LEN`（16） |
| 块高限频 | 距上次 publish ≥ `MinBlocksBetweenPublish` |

## 4. 存储

| 存储 | 说明 |
| --- | --- |
| `SyncAnchors` | `anchor_id → SyncAnchorRecord` |
| `ClearedAt` | clear 墓碑水位（永久保留，防复活；约 40B/锚） |

`SyncAnchorRecord`：`version` / `updated_at` / `ciphertext` / `depositor` / `deposit` / `last_publish_block`

## 5. 配置（`Config`）— runtime 当前值

| 项 | runtime 值 |
| --- | --- |
| `MaxAnchorLen` | 512 字节 |
| `MinBlocksBetweenPublish` | 100 块（~10 分钟 @ 6s；客户端 debounce 更长） |
| `AnchorDeposit` | 0.5 NEX（`UNIT / 2`） |
| `MaxClockSkew` | 3_600_000 ms（1 小时） |
| `ForceOrigin` | Root 或技术委员会多数 |

依赖 `pallet_timestamp` 提供 `now_ms` 时钟上界校验。

## 6. Runtime API 与 RPC

`runtime_api::ChatSyncApi`（只读、免费）：

| 方法 | 说明 |
| --- | --- |
| `sync_anchor(anchor_id)` | `(updated_at, ciphertext)` 或 `None` |

Node JSON-RPC：`chat_syncAnchor(anchorId, at?)` → `{ updatedAt, ciphertext(hex) }` 或 `null`
（`node/src/chat_rpc.rs`）。

**客户端恢复路径：** 助记词 → 重算 `anchor_pk` / `anchor_id` → `sync_anchor` → 本地解密清单 → 拉 blob。

## 7. 事件与错误

**事件：** `AnchorPublished` / `AnchorCleared` / `AnchorForceCleared`

**错误：** `BadAnchorSignature` / `AnchorNotFound` / `StaleUpdatedAt` / `UpdatedAtTooFarInFuture` /
`CiphertextTooShort` / `PublishTooFrequent`

## 8. 依赖关系

```
pallet-chat-common  ←── pallet-chat-sync（min_blocks_elapsed + deposit 薄封装）
pallet-timestamp    ←── 时钟上界校验
sp-io               ←── ed25519_verify、blake2_256
```

不依赖 `core` / `group` / `permission` / `inbox` / `msg-identity`。

## 9. 权重与基准

3 个 extrinsic 有 `WeightInfo` + `benchmarking.rs`（publish/clear 含 `ed25519_verify` 开销）。
已加入 runtime 基准清单。主网前应在参考硬件重跑 `runtime-benchmarks`。

## 10. 安全设计要点

1. **Mempool 抢跑首次 publish** — 仅「捐赠」押金给抢跑者；锚内容仍由锚密钥授权，无内容劫持。
2. **等值重发不重置限频** — 防止观察者用公开 (payload, sig) 活性骚扰。
3. **Clear 墓碑永久** — 主动 clear 后历史 publish 不可复活。
4. **Force-clear 非审查** — 用户可用更新清单重新 publish；仅清理遗弃/滥用锚。

## 11. 上线审计摘要（2026-06-19）

| 维度 | 结论 |
| --- | --- |
| **职责边界** | ✅ 仅密文清单；无明文/解密/inbox 耦合 |
| **授权模型** | ✅ 锚 Ed25519 签名 + 冻结 payload；付费与授权分离 |
| **LWW + 墓碑** | ✅ 防复活、防自锁、幂等 no-op |
| **限频/押金** | ✅ 块高间隔 + 首次押金 + 治理 force-clear |
| **Runtime 接线** | ✅ index 79 + API + `chat_syncAnchor` RPC |
| **权重 / 基准** | ✅ 含 ed25519 校验权重 |
| **单测** | ✅ 20 项（含跨语言签名向量、复活/重放/抢跑场景） |
| **缺口（非阻塞）** | ⚪ 押金终值待经济评审（ADR §11.5 标注）；⚪ 主网前重跑 benchmark；⚪ 独立 EISA ADR 文档已并入多设备总纲 |

**总评：达到上线标准。** EISA 核心安全属性（锚密钥授权、LWW、墓碑、限频、治理逃生门）均已
落地并有针对性单测覆盖。
