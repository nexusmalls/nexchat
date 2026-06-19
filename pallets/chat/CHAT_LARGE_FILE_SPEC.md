# Chat 大文件处理 · 链下方案与边界规范

> 状态：设计 / 边界决策（**链上不新增 extrinsic / storage**）
> 适用范围：`pallets/chat/{core,group,common}` 的文件类消息 + 链下投递/存储层
> 关联：链下 MLS payload 信封约定（relay 组件 / 客户端 `nexchat/`）、
> `core/src/lib.rs` §13 收敛说明、`pallets/storage/service/`（Pin 计费/多副本）、
> `common/media/`（媒体类型与大小上限）

## 0. 一句话结论

聊天大文件（图片 / 视频 / 语音 / 任意附件）**本体绝不上链、也不进 MLS payload**：
本体用**每文件独立对称密钥**加密后分块存入 IPFS；MLS 消息只携带**引用信封**
（`cid + file_key + 元数据`，几百字节）；链上最多见不透明 `cid`（人类消息全链下时连 `cid` 也不上链）。
持久化交给 `pallet-storage-service` 的 Tier 化多副本 Pin，或链下托管（Pinata / 自建）。

EN: Large chat files never touch the chain or the MLS payload. The file body is encrypted
with a **per-file symmetric key**, chunked, and stored on IPFS; the MLS message carries only a
small **reference envelope** (`cid + file_key + metadata`). Persistence is delegated to the
tiered Pin service (or off-chain hosting).

## 1. 设计前提（与现仓库一致）

- **链上只存 CID**：`chat-core` / `chat-group` 消息字段为 `content_cid: BoundedVec<u8, MaxCidLen>`，
  当前 `MaxCidLen = 96`。链不校验、不解密内容（审计 C）。
- **加密由客户端 MLS E2EE 保证**：链只存不透明字节。
- **媒体大小上限**（`common/media::limits`）：图片 50MB、视频 500MB、音频 100MB、CID ≤128B、
  视频时长 ≤1h、缩略图边 ≤320px。
- **持久化**：`pallet-storage-service` 按 `size_bytes × 副本数 × Tier 系数`（MiB 向上取整）计费，
  多运营者副本 + OCW 巡检 + 自动修复。

## 2. 核心原则

| 原则 | 说明 |
|---|---|
| 本体不上链 | 链上仅 CID（或全链下时不上链） |
| 本体不进 MLS payload | MLS application message 仅承载小负载；大文件撑爆 relay / epoch 处理 |
| 每文件独立 `file_key` | 随机生成，不复用会话密钥；支持单独转发 / 撤销而不泄露整段会话 |
| 密钥随消息经 MLS 下发 | 只有会话成员能解；relay / 链均不可见 `file_key` |
| 大文件分块 + manifest | 断点续传、并行下载、边下边播、坏块只重传单块 |
| 缩略图先行 | 列表只渲染缩略图，全量按需拉取（省带宽 + 护隐私） |

## 3. 文件信封约定（MLS payload 内，解密后结构）

文件类消息是链下 MLS payload 信封的一种 `type`，`body` 填本规范字段。
序列化用 CBOR / JSON（与客户端、relay 约定其一），链上对这些字段**完全无感**。

```jsonc
{
  "v": 1,
  "id": "<client-msg-id>",
  "type": "image|video|audio|file",
  "body": {
    "root_cid":   "Qm...",        // 单文件：密文 CID；分块：manifest 的 CID
    "chunked":    true,            // 是否分块（true 时 root_cid 指向 manifest）
    "file_key":   "<base64>",      // 该文件的对称密钥（AES-256-GCM）；随消息经 MLS 加密下发
    "mime":       "video/mp4",
    "size":       524288000,       // 原始字节数（计费 / 进度用）
    "file_sha256":"<hex>",         // 原始文件完整性校验
    "thumb_cid":  "Qm...",         // 可选：加密缩略图 / 首帧 CID
    "thumb_key":  "<base64>",      // 可选：缩略图密钥（可与 file_key 同，也可独立）
    "duration_ms": 360000,         // 可选：音视频时长
    "name":       "report.pdf"     // 可选：原始文件名（仅展示）
  },
  // 可选交互字段沿用 P3：reply_to / forward / ephemeral ...
  "ephemeral": { "ttl_ms": 86400000, "burn_on": "read" }
}
```

要点：
- **`file_key` 在 `body` 内**，整条信封经 MLS 加密；relay 与链均拿不到。
- **向后兼容**：未知字段忽略；非分块文件 `chunked=false` 时 `root_cid` 直指密文。
- **`id` 由客户端生成**（与 P3 一致），不依赖链上序号。

## 4. 分块与 Manifest

中小文件可整体单密钥加密后 `ipfs add`；**大文件（建议 > 16MB 或视频）必须分块**。

- 分块大小：默认 1MiB（客户端可配）。
- 每块：`AES-256-GCM(file_key, nonce = 块序号派生)` 独立加密，单独 `ipfs add`。
- Manifest（自身也加密上传，根 CID 进 `body.root_cid`）：

```jsonc
{
  "v": 1,
  "size": 524288000,
  "chunk_size": 1048576,
  "mime": "video/mp4",
  "file_sha256": "<hex>",
  "chunks": [
    { "cid": "Qm..", "nonce": 0, "sha256": "<hex>" },
    { "cid": "Qm..", "nonce": 1, "sha256": "<hex>" }
  ]
}
```

收益：断点续传、并行拉块、坏块只重下一块、视频可边下边播（按 manifest 顺序）。

## 5. 缩略图 / 预览

- 图片 / 视频：客户端生成小缩略图（`common/media::estimated_thumbnail_size`，边 ≤320px），
  独立加密上传得 `thumb_cid`，放入信封。
- 列表只渲染缩略图；用户点开才拉全量。
- 语音：可带 `duration_ms` + 波形摘要（小），按需拉本体。

## 6. 持久化与计费

**默认策略（2026-06，NexChat 已实现）**：

| 层级 | 聊天媒体 | sync blob（index/contacts/archive） |
|---|---|---|
| 链上 / 运营全局 Pin | **默认关**（`VITE_IPFS_PIN_ENABLED=false`） | §5.8 三层 pin（热/持久/灾备） |
| 发送方本机 kubo pin | **默认开** + TTL（`VITE_IPFS_MEDIA_LOCAL_PIN` / `…_TTL_MS`，默认 30 天）；1:1 接收方 `media_ack` 后收短为 1h 宽限 | 始终 `pin=true` |
| 阅后即焚 | `pin=false`，不登记 retention | — |
| 接收方 | 下载后本地 `MediaStore` 缓存；全量下载成功回发 `media_ack`（仅 1:1） | — |
| 用户「保留附件 ☆」 | 标星豁免清理 + 发送方移出 retention；LIVE 模式链上 Pin（正文 Temporary / thumb Standard） | — |

EN: Chat media is **not** globally pinned by default. Senders keep a **local kubo pin + TTL**
(`nexchat/src/ipfs/senderMediaRetention.ts`); receivers cache locally. Optional chain Pin remains
opt-in. Sync blobs stay on the §5.8 multi-layer pin path.

IPFS 上传 ≠ 永久。**可选**链上分级 Pin（用户显式开启或标记「保留附件」时）：

| 数据 | 建议 Tier | 副本 | 理由 |
|---|---|---|---|
| 缩略图 | Standard | 3 | 小、需长期可见 |
| 一般正文大文件 | Temporary | 1 | 大、按需、默认短周期 |
| 用户标记"重要" | Standard / Critical | 3 / 5 | 升级保留 |
| 证据 / 合规文件 | Critical | 5 | 法律留痕（走 evidence 域） |

落地（**opt-in** 链上留痕）：发送方对各 CID 调 `request_pin_for_subject(subject_id, cid, size_bytes, tier)`；
到期 `renew_pin` 续费，否则宽限后 unpin。

省成本策略：
- 默认依赖**发送方本机 TTL + 接收方本地缓存**；过期后显示「文件已过期，请对方重发」。
- 大文件链上 Pin 默认 **关**；开启时用 **Temporary + 短周期**。
- 阅后即焚文件：relay 设 TTL + **不 Pin**（对齐 P3 ephemeral）；到期 relay 不再投递、客户端本地销毁。

## 7. 与"隐藏通信关系 / 全链下"方案的衔接

若采用社交图谱 + 人类消息全链下方案（见隐私设计）：
- **链上不出现 `cid`**：`cid` / `file_key` / manifest 全在 MLS 信封内，relay 仅转密文。
- 不希望暴露"谁 Pin 了什么"时，用 **Pinata / 自建 IPFS** 持久化，而非链上 `request_pin`
  （链上 Pin 会留下 `cid` + payer）。
- **换机恢复**：大文件的 `file_key` 与 manifest 根 CID 须写入会话 / 消息加密备份 vault，
  否则换机后旧文件无法解密（见通讯录 / 会话备份规范）。

## 8. 大小上限与防滥用

| 控制点 | 约定 |
|---|---|
| 单文件硬上限 | 复用 `common/media`：图 50MB / 视频 500MB / 音频 100MB |
| 单块上限 | 强制分块；拒绝单块 > N MiB（建议 ≤4MiB） |
| relay | 只转 MLS 控制消息（小）；文件块走 IPFS，本体不经 relay 转发 |
| 频率 | 客户端 / relay 限频；链上 Pin 滥用由 `storage-service` 计费 + OCW 约束 |

## 9. 端到端流程（发大视频为例）

发送方：
1. 生成随机 `file_key`。
2. 视频按 1MiB 分块，每块 `AES-GCM` 加密 → `ipfs add` → 收集块 CID。
3. 组 manifest（块列表 + hash）→ 加密 → `ipfs add` → `root_cid`。
4. 生成缩略图 / 首帧 → 加密 → `thumb_cid`。
5. 发 MLS 文件信封：`{type:"video", root_cid, file_key, thumb_cid, size, duration_ms}`。
6. （可选）`request_pin_for_subject(root_cid + 各块, Temporary)` 或走链下托管。

接收方：
7. MLS 解密消息 → 得 `file_key + root_cid + thumb_cid`。
8. 先拉 `thumb_cid` 解密显示缩略图。
9. 点开 → 拉 manifest → 并行拉各块 → `file_key` 解密 → 校验 hash → 播放。

## 10. 链上明确"不做"清单（及理由）

| 不做的事 | 理由 |
|---|---|
| 链上存文件本体 / 分块字节 | 区块膨胀 + 破坏 E2EE，违背"链上仅元数据" |
| 链上存 `file_key` / manifest | 任何明文密钥上链即等于不加密 |
| 大文件经 MLS payload 传输 | 撑爆 relay / epoch 处理；MLS 仅传引用 |
| 链上文件级 reaction / 进度状态机 | 高频、强隐私，链上承载无收益（见 P3） |
| 为大文件新增 chat extrinsic / storage | 维持 §13 收敛；本体与密钥均链下 |

## 11. 交付边界

- **链上侧（本仓库）**：不引入新 extrinsic / storage；`chat-core` / `chat-group` 维持仅 `content_cid`。
  持久化复用 `pallet-storage-service` 既有接口。
- **链下侧（客户端 + relay）**：按 §3 信封、§4 分块 / manifest、§5 缩略图、§9 流程实现；
  持久化按 §6 选择链上 Pin 或链下托管。
- 如链下实现过程中确需链上原语，回到 `core/src/lib.rs` §13 链上/链下边界单独评审。
