# Chat 设备端保留与清理策略 · 设计草案

> 状态：设计草案（**客户端 + 可选 relay 职责，链上不参与**）
> 适用范围：聊天客户端本地存储（私聊 + 群聊），与 relay/IPFS 协同
> 关联：`CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md`（阅后即焚/懒加载即本草案的子能力）、
> `core` 的链上 180 天元数据过期（`MessageExpirationTime`）

## 0. 一句话目标

让设备本地聊天历史**可控、可预算、可恢复**：默认体验"消息不丢"，但提供按会话/按时间/按
容量的保留与自动清理，使本地占用有上界；被清理的内容在密钥仍可用时可从 IPFS/relay 重新拉回。

## 1. 边界与定位

- **链上不管设备保留**：链上私聊元数据有 180 天软过期（且需双方删除才回收），但那是节点存储，
  不是用户设备；群消息根本不上链。设备本地"存多久/占多大"**完全是客户端策略**。
- **本草案是客户端规格**：定义本地数据模型、保留维度、清理触发与算法、恢复路径与安全交互。
  本仓库的 pallet **不**新增任何 extrinsic/storage 来实现它。
- **relay 为可选协同方**：负责离线补投窗口与 ephemeral 到期不再投递；不是设备保留的必需件。

## 2. 设备本地存的是什么

| 数据 | 说明 | 体积特征 | 可否远端恢复 |
|---|---|---|---|
| 会话索引 / 消息元数据 | 会话列表、消息时间线、收发态、CID 指针、引用关系 | 小、线性增长 | 部分（链上私聊元数据可重建；群需本地/relay） |
| 解密后正文缓存 | 文本/结构化内容解密结果 | 中 | 需密钥 + 密文（IPFS/relay） |
| 媒体 blob | 图片/视频/文件原文 | **大头** | CID 在则可重拉（IPFS Pin 在的前提下） |
| MLS 群状态/密钥材料 | epoch/ratchet/私钥 | 小但敏感 | **不可**随意删（删了无法解历史/参与群） |

> 关键区分：**元数据/索引** 与 **媒体 blob** 必须分开管理——媒体是占用主因，应可单独更激进地清理。

## 3. 保留维度（可叠加）

每个会话的有效保留 = 以下规则取**最严**（除"豁免"项）：

1. **按时间**：保留最近 `retention_days`（默认"永久"=0 表示不按时间清）。
2. **按条数**：每会话最多保留 `max_messages`（默认不限）。
3. **按容量**：全局本地配额 `device_quota_mb`，超限按 LRU 淘汰（默认开，给一个保守上限）。
4. **媒体单独策略**：`media_auto_download`（Wi-Fi/全部/手动）+ `media_retention_days`
   （媒体可比正文更短，例如正文永久、媒体 30 天后仅留缩略图 + CID）。
5. **阅后即焚覆盖**：带 `ephemeral.ttl` 的消息（见 P3 信封）到期**强制**本地销毁，**优先级最高**，
   不受"永久保留"影响。
6. **豁免**：用户**收藏/标星**或**置顶**的消息、以及**未同步成功**的本地待发消息，永不被自动清理。

## 4. 清理触发

| 触发 | 时机 | 作用 |
|---|---|---|
| 定时扫描 | 应用启动 + 每 N 小时空闲时 | 执行时间/条数/媒体到期规则 |
| 容量水位 | 写入后超过 `device_quota_mb` 高水位 | LRU 淘汰至低水位（先媒体后正文） |
| 即焚 TTL | 消息读后/送达后计时到期 | 强制销毁该条 |
| 手动 | 用户"清理缓存/删除某会话历史" | 立即执行，可选保留索引 |
| 账户事件 | 登出/解绑设备 | 按隐私级别清本地（密钥与明文优先） |

## 5. 分层存储与恢复

采用 **热/冷分层**，使"清理"尽量是"降级"而非"丢失"：

- **热层**：会话索引 + 元数据 + 最近正文（始终保留，体积小）。
- **冷层（可卸载）**：媒体 blob、久远正文缓存。清理时**只删 blob、保留 CID 指针 + 缩略图**，
  条目在 UI 仍可见为"点击重新下载"。
- **恢复路径**：用户回看时按 CID 从 IPFS/relay 重新拉取并用本地密钥解密，回填热层。

```
渲染请求 ──> 命中本地? ──是──> 直接显示
                │否
                └─> 有 CID + 密钥? ──是──> 从 IPFS/relay 拉密文 → 解密 → 回填 → 显示
                                    │否
                                    └─> 显示"已清理且不可恢复"占位（见 §6）
```

## 6. 与安全/隐私的交互（重要）

- **不可恢复是特性，不是缺陷**：MLS 前向保密下，密钥轮换后旧 epoch 内容无法用新密钥解；
  阅后即焚清掉本地明文后即真正不可读。本地清理 + 密钥不可用 = **彻底销毁**，符合隐私目标。
- **因此恢复有前提**：只有"CID 可取 + 对应解密密钥仍在本地/可重建"时才能重拉。即焚消息、已轮换
  且未留存密钥的历史，**设计上就不该恢复**，UI 应明确提示而非误导用户"还能找回"。
- **多设备**：新设备只能看到其加入 epoch 之后、或经显式历史共享授权的内容；本地保留策略**不应**
  绕过 MLS 历史可见性。

## 7. 本地数据模型草图（客户端，示意）

```sql
-- 会话级保留配置（覆盖全局默认）
conversation(
  conv_id, kind/*direct|group*/, pinned BOOL,
  retention_days INT NULL, max_messages INT NULL,         -- NULL=继承全局
  media_retention_days INT NULL
)

-- 消息条目（元数据常驻热层）
message(
  conv_id, msg_id/*client id*/, sent_at, sender_ref,
  type, body_cache NULL/*可空=已卸载*/, content_cid,
  reply_to NULL, ephemeral_burn_at NULL,                  -- 来自 P3 信封
  starred BOOL, sync_state/*pending|sent|acked*/,
  last_access_at                                          -- 供 LRU
)

-- 媒体 blob（冷层，可单独淘汰）
media_blob(content_cid, conv_id, bytes_path NULL, thumb_path, size_bytes, last_access_at)
```

## 8. 清理算法（单次 pass，伪代码）

```text
fn cleanup_pass(now, cfg):
  # 1) 即焚（最高优先，强制）
  delete messages where ephemeral_burn_at != NULL and ephemeral_burn_at <= now
        (含 body_cache 与 media_blob，彻底删)

  # 2) 时间/条数（按会话；豁免 starred / pinned / sync_state=pending）
  for conv in conversations:
    r = effective_retention(conv, cfg)
    drop_body_and_media(messages in conv older than r.retention_days, except exempt)
    keep newest r.max_messages bodies in conv, offload older (保留元数据 + CID)

  # 3) 媒体单独到期
  offload media_blob older than media_retention_days  # 删 blob 留 thumb+CID

  # 4) 容量水位（LRU，先媒体后正文，绝不动 exempt）
  while local_size() > cfg.quota_high:
    evict least-recently-accessed cold-layer item down to quota_low
```

不变式：**永不删除** `starred / pinned / sync_state=pending` 的内容；清理优先"降级到冷层/卸载
blob"，仅即焚与显式删除才物理移除元数据。

## 9. 默认值建议（保守、体验优先）

| 配置 | 默认 | 说明 |
|---|---|---|
| `retention_days`（正文） | 0（永久） | 默认不按时间删正文 |
| `max_messages`/会话 | 不限 | |
| `device_quota_mb` | 给一个保守上限（如 2–4 GB，平台相关） | 超限才 LRU |
| `media_auto_download` | 仅 Wi-Fi | 控制媒体落地量 |
| `media_retention_days` | 30（到期留缩略图 + CID） | 媒体是占用主因 |
| 即焚 | 跟随发送方 `ephemeral.ttl` | 接收端无条件执行 |
| 豁免 | starred / pinned / 未发送成功 | 永不自动清 |

## 10. 客户端可调 UX（设置项）

- 全局：本地存储用量看板、一键"清理缓存（保留索引）"、设备配额滑杆。
- 按会话：保留时长（永久/30/90 天）、媒体自动下载、"清空此会话历史"。
- 即焚：发送方设置 TTL；接收端展示"阅后即焚"标记与倒计时。

## 11. 链上/客户端职责边界（交付清单）

- **链上（本仓库，已具备，无需新增）**：私聊元数据 180 天软过期 + 治理 GC、群禁言/封禁/冻结状态、
  `anchor_message_digest` 审计锚、P2 举报。这些**不驱动**设备保留。
- **客户端（按本草案实现）**：本地数据模型、保留维度、分层存储、清理算法、恢复与即焚执行、设置 UX。
- **relay（可选）**：离线补投窗口、对 ephemeral 到期停止补投；不承担设备保留。

## 12. 非目标

- 不在链上记录设备保留/清理事件（违背隐私，且无意义）。
- 不做"云端聊天记录托管/备份"——如未来需要，应是**端到端加密的显式备份**特性，单独立项，不属本草案。
- 不绕过 MLS 历史可见性来"找回"本不该可见的历史。
