# ADR · 云同步双轨恢复 · Relay 运维备份 + 链上加密 Sync Anchor (EISA)

> **Status / 状态**：Proposed v2.2（待评审 / pending review）  
> **Date / 日期**：2026-06-10  
> **Deciders / 决策方**：Chat pallet + NexChat client + Relay ops  
> **Supersedes / 关系**：**不推翻** `CHAT_P2_SESSION_ANCHOR_DESIGN.md`（仍否决链上会话关系锚）；**补充** P2 §2.1「指针不上链（明文）」为 **账户派生加密锚（AnchorId）** + **Relay 运维整库备份** 并行方案。  
> **Revision / 修订**：v2.2——链上键由 `InboxId` 改为助记词可重算的确定性 `AnchorId`；授权改为锚密钥签名；两项硬前置：`vault_master` 换根（§5.0）+ blob 多点存活（§5.8）；自愈承诺收敛为「数据层自愈」（§6.5）；v2.2 补齐开发级字节合同与实现约束；修订记录见 §15。

---

## 0. TL;DR

CN: NexChat 云同步采用 **三层并行**，职责不重叠：

| 层 | 名称 | 职责 |
|----|------|------|
| **A** | Relay 热 KV | 日常低延迟 `*_put` / 投递 / spent |
| **B** | 运维备份 | `RELAY_DATA_DIR` 加密 tarball → IPFS pin（换机 **全员、含 spent**） |
| **C** | EISA | `anchor_id → AES-GCM(SyncManifest)` 上链；`anchor_id` 凭助记词可重算；Relay 空库时 **数据层登录自愈**（边界见 §6.5，blob 可用性前置见 §5.8） |

**否决**作为默认方案的：`AccountId → 明文 CID` 链上索引；v1 草案的 `InboxId` 键控亦已否决（§9）。

EN: Off-chain cloud sync uses **three parallel layers**: Relay hot cache (A), ops backup of full `data/` (B), and an account-derived **Encrypted Sync Anchor** on-chain (C; the acronym EISA is retained from v1). Plain `AccountId → CID` on-chain is **rejected** for default.

---

## 1. Context / 背景

### 1.1 现有架构

- **加密 blob**（conv-index / contacts-vault / msg-archive）在 **IPFS**，密钥 `K_index` / `K_contacts` / `K_archive` 由账户主密钥 HKDF 派生（NexChat `keyVault`）。
- **指针** `{ cid, updated_at }` 在 **Relay**（`index_put`, `contacts_put`, `msg_archive_put`），持久化 WAL + snapshot（`nexchat/docs/RELAY_PERSISTENCE.md`）。
- **链上**不参与人类消息正文；`CHAT_P2` 否决链上私聊会话锚（who↔whom 明文）。

### 1.2 待解决问题

| 问题 | 仅 Relay | 需要 |
|------|----------|------|
| 换 Relay 服务器 | 拷 `data/` 或丢指针 | **B**：运维自动化 |
| 运维未 restore、Relay 空库上线 | 用户丢云端索引 | **C**：用户层自愈 |
| 用户换机（无 localStorage） | 需 Relay 或链 | **C** + IPFS |
| spent / mailbox | Relay 独有 | **B** 整库 restore |

### 1.3 与本 ADR 无关的目标

- 不把 MLS 群状态、消息正文、Blind-RSA 私钥上链。
- 不用链上 KV 替代 IPFS pin 激励（`pallet-storage-service` 仍独立）。
- 不恢复 P2 已否决的 `touch_session(peer)` 类链上关系索引。

---

## 2. Decision / 决策

采用 **A + B + C 并行**，默认产品档位 **Standard**（B 运维必做 + C 长 debounce 异步写链，§11.2）。

```text
         ┌──────── IPFS 加密 blob（不变）────────┐
         │  conv-index / contacts / msg-archive   │
         └───────────────────┬────────────────────┘
                             │ cid in SyncManifest
     ┌───────────────────────┼───────────────────────┐
     ▼                       ▼                       ▼
 Layer A              Layer B                 Layer C
 Relay KV             Ops backup              EISA (chain)
 account→{cid,ts}     data/ tarball           anchor_id→ciphertext
 热路径 + spent        GPG + IPFS pin          K_sync 加密 SyncManifest
```

---

## 3. Layer A — Relay（保持现状）

无协议变更；参考 `nexchat/scripts/relay-server.mjs` + `RELAY_PERSISTENCE.md`。

**Non-goals for migration**：A  alone 不能保证换机；B/C 补充。

---

## 4. Layer B — 运维备份（Ops backup）

### 4.1 范围

备份 **整个** `$RELAY_DATA_DIR`：

- `relay-state.json`, `relay-journal.ndjson`, `.bak`
- 含：三类指针、inbox 注册、spent、mailbox 缓存

### 4.2 工具与周期

- 脚本：`nexchat/scripts/relay-backup-to-ipfs.sh`（GPG 对称加密 → `ipfs add --pin` → `latest-backup.json`）**（待实现）**
- 恢复：`nexchat/scripts/relay-restore-from-ipfs.sh`**（待实现）**
- 周期：**15min～daily**（生产建议 systemd timer + 换机前强制一次）
- 可选：`BACKUP_STOP_RELAY=1` + `systemctl stop/start` 保证快照一致
- **GPG 口令管理**：口令不得只存 Relay 本机；至少异地两份（运维密管 + 离线备份）；轮换时旧口令保留至其加密的最后一份备份过期
- **`latest-backup.json` 异地存放**：备份 CID 必须写到 Relay 主机之外（S3 / 第二主机 / DNS TXT 任一），否则主机整机故障时无法定位备份

### 4.3 生产换机 SOP（主路径）

1. 最后一次 backup  
2. `SIGTERM` 停旧 Relay → restore `data/` 到新主机  
3. 启 Relay → `relay-sync-audit.mjs` / `admin_stats` 验收  
4. 切 DNS / `VITE_RELAY_WS`  

**此路径不依赖 C**；全员即时恢复，含 spent。

> **spent 重放窗口**：若被迫用落后于停机点的旧备份恢复，时间差内已花费的令牌可被重放。SOP 默认「先 backup 后停机」将窗口压到 ~0；使用旧备份时应公告受影响用户 bump inbox epoch。

---

## 5. Layer C — EISA（Encrypted Sync Anchor，账户派生加密同步锚）

> 缩写 EISA 沿用 v1（原 Encrypted **Inbox** Sync Anchor）；v2 起键不再绑定投递 inbox。

### 5.0 硬前置：`vault_master` 换根（不满足则 C 层不得上线）

现状 `keyVault.init(selfAddress)`（`nexchat/src/state/appStore.ts`）的 HKDF 基钥 = `SHA-256(SS58 地址)`，**任何人可算**。若在此前提下上线 EISA，`anchor_id` 与 `K_sync` 全网可派生：锚可被任意定位、密文可被任意解密，本 ADR 的隐私论证全部失效（现有 `K_index` / `K_contacts` / `K_archive` 同样受此影响，属同一漏洞）。

**前置任务**（NexChat，列入 §10 P0）：

```text
vault_master = HKDF-SHA256(
  ikm  = sr25519_secret(64B),       // 解锁后的 keyring pair secret；与助记词等价、跨设备确定
  salt = "nexchat/vault-master/v1",
  info = ss58(prefix 42)            // 账户域分离（多账户共用助记词时不共 master）
)
```

- **`sr25519_secret` 提取路径（统一规则）**：从**已解锁的 `KeyringPair`** 提取——`pair.encodePkcs8()`（无参=不加密）按 PKCS8 固定布局解出 64B expanded secret。该规则对所有账户来源一致：mnemonic 创建、keystore JSON 导入（**不依赖**单独存储的加密助记词记录）、dev 模式 URI seed（`//Alice` 派生的 pair 同样适用）。PKCS8 解包封装为独立函数 + 固定测试向量（防 polkadot-js 布局变更静默破坏）。mock/测试环境经显式 `initForTest(seed)` 注入，生产路径 `init` 必传。
- `keyVault.init` 改为接收 `vault_master`（非可导出 CryptoKey），并补 `clear()`（锁定 / 切账户时清除）；移除生产路径的演示种子默认值。
- 存量数据迁移：**旧钥由公开地址可重算**，故本地库与三类 IPFS blob 可全自动「旧钥解密 → 新钥重封」，无需用户参与。blob wire 头部加 1 字节版本号（`0x02`）；旧格式无版本字节，解析时**先按 v2 头尝试、GCM 认证失败回退旧格式**，迁移完成后只写 v2。
- 派生规范一旦有锚上链即**永久冻结**；客户端固定测试向量保证跨平台（web / 桌面 / 移动）一致。

### 5.1 原则

| 规则 | 说明 |
|------|------|
| 链上键 | **`AnchorId`**（32B）= `blake2_256(anchor_pk)`，由 `vault_master` 确定性派生（§5.3）；**任何设备凭助记词可重算**。不是 `AccountId`，也不是 `InboxId` |
| 链上授权 | **锚签名**（`anchor_sig`，Ed25519，§5.5）；extrinsic 的签名 origin 仅承担手续费与押金，不参与授权 |
| 链上值 | **仅密文** + `updated_at` + `version`；**不存明文 CID** |
| 链下明文 | `SyncManifest` JSON，仅客户端解密后可见 |
| 频率 | 经统一 **`OffchainSyncCoordinator`**（§14.6）：**Relay 短 debounce**（3–15s，§14.3）与 **链锚长 debounce**（生产默认 ≥15min，及 sign-out / manual，§14.3）**分离**；每 anchor 每长窗口 **最多 1 笔** extrinsic；SyncManifest **字节不变则跳过** extrinsic |
| 写顺序 | **local + Relay 先落盘**（不阻塞发消息）；链锚 **异步** 提交，失败入队重试（§6.1） |
| 与 P2 关系 | 不上链会话关系；不上链明文指针；是 **可移植加密书签** |

**Debounce 分层（避免与 §14 冲突）**：

- **短窗口（Relay / local / IPFS）**：数据变更后合并上传 blob 并 `*_put`；debounce 见 §14.3 左列。  
- **长窗口（链锚）**：仅在 SyncManifest **相对上次已上链版本有变化** 时，且距上次 `publish_sync_anchor` ≥ 客户端链 debounce（§14.3 右列），或 sign-out / 用户手动「备份到链」时提交。  
- **链上硬顶**：`MinBlocksBetweenPublish`（§5.4）为 spam 防护，与客户端长 debounce **取较严者**生效（见 §11.2）。

**与投递 inbox 彻底解耦**：`anchor_id` 不含任何 inbox 成分。`pallet-chat-inbox::bump_epoch`、inbox 重注册、RSA 密钥重生成都**不影响锚**；`pallet-chat-inbox` 的「controller 必须为一次性密钥」不变量完整保留，sealed-sender 的接收方匿名性不被 sync 功能触碰。

**v1 付费方（见 §11.1）**：`publish_sync_anchor` 由 **主聊天 `AccountId`** 签名并支付手续费 / 押金（链上可见付费关联，§5.7 如实披露）；**授权始终由 `anchor_sig` 决定**。v2 可将付费方换成 proxy / 一次性账户 / 赞助费率以断开关联——存储与授权模型零迁移。

### 5.2 SyncManifest（客户端明文，不上链）

```jsonc
{
  "v": 1,
  "updated_at": 1738665600000,
  "index":    { "cid": "bafy…", "updated_at": 1738665600000 },
  "contacts": { "cid": "bafy…", "updated_at": 1738665590000 },
  "archive":  { "cid": "bafy…", "updated_at": 1738665580000 }
}
```

- 某类未启用时可省略字段；**跨来源合并按字段逐项 LWW（规则见 §6.2）**，顶层 `updated_at` 仅作缓存失效提示。
- `cid` 为 IPFS 根 CID（与现有 `*Pointer` 一致）。
- **规范化序列化（双端合同）**：加密与 hash-skip 均作用于 **canonical JSON 字节**——UTF-8、对象键按字典序排序、无空白、数字不带多余零。否则同一内容在不同设备产生不同字节 → 「字节不变则跳过」失效、空发 extrinsic。规则与测试向量随 §5.0/§5.5 向量文件一并冻结。

### 5.3 密钥派生（全部以 §5.0 `vault_master` 为根）

```text
anchor_seed = HKDF-SHA256(ikm = vault_master, salt = "chat/sync-anchor-key/v1")
(anchor_sk, anchor_pk) = Ed25519-keygen(anchor_seed)
anchor_id   = blake2_256(anchor_pk)                       // 32B，链上存储键
K_sync      = HKDF-SHA256(ikm = vault_master,
                          salt = "chat/sync-manifest/v1",
                          info = anchor_id)               // 域分离，为多锚预留
```

- 加密：`AES-256-GCM`，wire = `iv(12) || ciphertext`（与 `encryptIndexBlob` 同构）。
- Ed25519-keygen：以 `anchor_seed`（32B）为标准 Ed25519 seed（RFC 8032），JS 侧用 `@noble/ed25519` 或等价库；**JS 与 Rust（`sp_core::ed25519`）共享固定测试向量**（seed → pk → 对样例 payload 的签名）。
- 全链路确定性：助记词 → pair secret → `vault_master` → 锚密钥 / 锚 ID / `K_sync`，新设备零外部依赖重算（这是 v1 草案 `InboxId` 键控做不到的，见 §9）。
- 多锚扩展：未来在 `anchor_seed` 的 salt 中加序号（`…/v1/0`、`…/v1/1`）即可；v1 仅单锚。

### 5.4 链上存储（新 pallet `pallet-chat-sync`）

```rust
/// EN: Account-derived opaque anchor key: `anchor_id = blake2_256(anchor_pk)`.
/// The chain never learns which account derives it; authorization is by an
/// Ed25519 signature of the anchor key, not by the extrinsic origin.
/// CN: 账户派生的不透明锚键：`anchor_id = blake2_256(anchor_pk)`。链不知道它由哪个
/// 账户派生；授权依据锚密钥的 Ed25519 签名，而非 extrinsic origin。
pub type AnchorId = [u8; 32];

#[pallet::storage]
pub type SyncAnchors<T: Config> = StorageMap<
    Blake2_128Concat,
    AnchorId,
    SyncAnchorRecord<T>,
    OptionQuery,
>;

pub struct SyncAnchorRecord<T: Config> {
    pub version: u8,                                  // = 1
    pub updated_at: u64,                              // manifest.updated_at, ms
    pub ciphertext: BoundedVec<u8, T::MaxAnchorLen>,  // suggest MaxLen = 512
    pub depositor: T::AccountId,                      // 押金支付方（clear 时退还）
    pub deposit: BalanceOf<T>,
    pub last_publish_block: BlockNumberFor<T>,        // MinBlocksBetweenPublish 依据
}

/// EN: Clear tombstone watermark (`anchor_id -> updated_at at clear time`):
/// publishing onto a cleared anchor requires a STRICTLY newer `updated_at`,
/// otherwise anyone could resurrect it by replaying a historical publish
/// (payload, sig) from chain history. Kept forever (~40B per cleared anchor).
/// CN: clear 墓碑水位（`anchor_id -> clear 时的 updated_at`）：向已 clear 的锚
/// publish 必须携带**严格更大**的 `updated_at`，否则任何人可用链历史中的 publish
/// (payload, sig) 重放复活。永久保留（每个被 clear 的锚约 40B）。
#[pallet::storage]
pub type ClearedAt<T: Config> = StorageMap<Blake2_128Concat, AnchorId, u64, OptionQuery>;
```

Pallet 带 `storage_version(1)`（为未来迁移留版本基线）。

**Config 建议**：

- `MaxAnchorLen`: 512（留 growth headroom）
- `MinBlocksBetweenPublish`: 100（~10min @ 6s block；**链上硬顶**，非目标写入频率——目标频率见 §14.3 客户端长 debounce）
- `AnchorDeposit`: 固定小额（量级同 `pallet-chat-inbox` 押金，§11.5）
- `MaxClockSkew`: 1h（`updated_at` 上界容差，防自锁，§5.5）

> v1 草案的 `MaxAnchorsPerBlock` 已移除：交易池无法按全局配额协调，拥塞时只会造成随机失败；块重量 + per-anchor 块高间隔 + 手续费已足够。

### 5.5 Extrinsic

```rust
/// EN: Publish or replace the encrypted sync anchor at `blake2_256(anchor_pk)`.
/// Authorization = Ed25519 `anchor_sig` by the anchor key; the signed origin
/// only pays fees/deposit. Rejects stale (LWW) and far-future `updated_at`.
/// CN: 发布/更新 `blake2_256(anchor_pk)` 处的加密同步锚；授权 = 锚密钥的 Ed25519
/// 签名，签名 origin 仅付费/押金；过期与超前时间戳均拒绝。
#[pallet::call]
pub fn publish_sync_anchor(
    origin,                                        // 付费方；v1 = 主账户
    anchor_pk: [u8; 32],
    updated_at: u64,
    ciphertext: BoundedVec<u8, T::MaxAnchorLen>,
    anchor_sig: [u8; 64],
) -> DispatchResult

/// EN: Remove the anchor and refund the deposit to `depositor`.
/// CN: 删除锚并向 `depositor` 退还押金。
pub fn clear_sync_anchor(
    origin,
    anchor_pk: [u8; 32],
    anchor_sig: [u8; 64],                          // 签名绑定 stored.updated_at，防重放
) -> DispatchResult

/// EN: Governance escape hatch (ForceOrigin): remove an anchor without an anchor
/// signature — the anchor key derives from a mnemonic and may be lost forever.
/// Deposit refunds to the depositor; tombstone is set as in a regular clear.
/// Cannot censor future state: the owner can always re-publish newer.
/// CN: 治理逃生门（ForceOrigin）：无锚签名移除锚——锚密钥派生自助记词，可能永久
/// 丢失。押金退还 depositor；与常规 clear 一样写墓碑。无法审查未来状态：持有者
/// 随时可重新发布更新清单。
pub fn force_clear_sync_anchor(origin: ForceOrigin, anchor_id: AnchorId) -> DispatchResult
```

**签名 payload 字节级编码（双端合同，定后冻结）**：

```text
payload = UTF8(context)                       // 变长，常量字符串
        ‖ genesis_hash      (32B)
        ‖ anchor_id         (32B)
        ‖ updated_at        (u64, little-endian, 8B)   // clear 时为 stored.updated_at
        ‖ blake2_256(ciphertext) (32B)                 // clear 时省略此段

context = "nexus/chat-sync/publish/v1"  |  "nexus/chat-sync/clear/v1"
```

裸字节拼接、无长度前缀（context 为编译期常量，其余字段定长）；`updated_at` 统一 **LE**。JS（`syncAnchor.ts`）与 Rust（pallet）各自实现，以**共享测试向量**钉死（向量文件随 §5.0 派生向量一并维护）。

**publish 校验**：

1. `anchor_id = blake2_256(anchor_pk)`（存储键由链计算，调用方不可指定）  
2. `ed25519_verify(anchor_sig, payload, anchor_pk)`（payload 编码见上）  
3. `ciphertext.len()` ∈ `[16, MaxAnchorLen]`（非空，上限）  
4. LWW：`updated_at` ≥ stored.updated_at（**`==` 允许**：幂等重发/同 ts 覆盖是有意语义，授权已由签名保证）  
   4b. **等值重发 = 幂等 no-op**：`updated_at` 与密文均与存储一致时，不写状态、**不重置限频时钟**，仅发事件后返回成功——否则任何观察者可逐块重放公开的 (payload, sig) 把 `last_publish_block` 顶到最新，使持有者的真实更新永远赶不上窗口（限频骚扰）  
   4c. **clear 墓碑**：锚不存在但 `ClearedAt` 有水位时，要求 `updated_at` **严格大于**水位——历史 publish 重放不能复活被主动 clear 的锚  
5. 上界：`updated_at` ≤ `pallet_timestamp::now()` + `MaxClockSkew`（防 `u64::MAX` 自锁——锚密钥派生自助记词、不可轮换，被盗后无换钥自救路径，必须由链兜底）  
6. 块高 rate limit：`current_block - last_publish_block ≥ MinBlocksBetweenPublish`（per anchor，仅对**实际写入**生效，见 4b）  
7. 首次发布：从 `origin` reserve `AnchorDeposit`，记录 `depositor`；**后续 publish 不论 origin 是谁，`depositor` 与押金保持不变**（clear 时退还原 depositor）

**clear 校验**：同 1–2（payload 用 clear context + `stored.updated_at`，绑定当前存储值，签名不可跨状态重放）；锚不存在 → `AnchorNotFound`；押金退还 `depositor`；**写入 `ClearedAt[anchor_id] = stored.updated_at` 墓碑**（使 ≤ 该值的全部历史 publish 签名永久失效）。`force_clear_sync_anchor` 同样退押金 + 写墓碑，但由 `ForceOrigin` 授权、发独立事件。

**Events / Errors（实现外观面）**：

```rust
Event:  AnchorPublished    { anchor_id, updated_at },   // 前端确认上链依据
        AnchorCleared      { anchor_id },
        AnchorForceCleared { anchor_id },                 // 治理透明（独立于常规 clear）
Error:  BadAnchorSignature, AnchorNotFound, StaleUpdatedAt, UpdatedAtTooFarInFuture,
        CiphertextTooShort, PublishTooFrequent, /* + BoundedVec 长度由类型系统保证 */
Config: Currency（押金）、ForceOrigin（治理逃生门）、WeightInfo（含 ed25519_verify 基准）
```

**不校验** ciphertext 内容（链不解密）。

**抢注 / 抢跑分析**：不存在 FCFS 窗口——`anchor_id` 在首次发布前不可猜测（派生自秘密 `vault_master`）；发布后即便复制 `anchor_pk` 也无法**伪造新内容**的 `anchor_sig`。mempool 观察者用**篡改后的载荷**抢跑首笔交易无效（签名校验失败）；用**原封不动的 (payload, sig)** 抢跑则会成功落账，但效果仅是替持有者垫付押金（`depositor` 记为抢跑者）、内容恰为持有者所签——持有者自己的交易随后按 4b 以幂等 no-op 落账，后续更新不受影响。两种情形均无需首写绑定状态机（配套单测：`mempool_front_run_of_first_publish_only_donates_deposit`）。

### 5.6 Runtime API / RPC

```rust
fn sync_anchor(anchor_id: AnchorId) -> Option<(u64, Vec<u8>)>; // updated_at, ciphertext
```

Node 封装：`chat_syncAnchor(anchorId)` → 前端 `chainClient` 只读。

### 5.7 链上可见面（隐私披露）

| 可见 | 不可见 |
|------|--------|
| anchor_id 有/无 anchor、更新节奏 | 明文 CID、SyncManifest 内容 |
| 密文长度 | 会话对端、消息正文 |
| extrinsic 历史时间线 | 投递 inbox 关联（与 `pallet-chat-inbox` 无键复用） |
| **付费账户（v1 = 主账户）** | 由 anchor_id 反推账户（除上述付费关联外不可行） |

**v1 诚实披露**：只要主账户付费，extrinsic 历史即泄漏「账户 X 在用云同步 + 更新节奏 + 密文长度」，与 `AccountId → 密文` 方案等价。AnchorId + 锚签名的价值在于把该泄漏限制在**付费层**：v2 换 proxy / 一次性账户 / 赞助费率即可断开关联，存储与授权零迁移（§11.1）。

**优于** `AccountId → CID`：无全网明文 CID 索引。**优于** v1 草案 `InboxId` 键控：新设备凭助记词可重算键（恢复闭环成立）；不触碰投递 inbox 匿名性。

**弱于** 纯 Relay：链上仍有 **锚活跃度** 元数据 → 设置 **Relay-only** 档位（见 §7）。

### 5.8 Blob 可用性：sync blob 多点存活（C 层第二硬前置）

**问题**：链锚只存 CID，不存数据。现状 blob 上传只打到单一 `VITE_IPFS_API_URL` 节点（`pin=true` 也只 pin 在该节点），`pallet-storage-service` 链上 pin 默认关闭且从未覆盖 sync blob。**「Relay 空库」的现实成因（主机丢失、磁盘损坏）大概率同时摧毁同机 IPFS 节点**——C 层兜底的场景恰好是其数据源最可能消失的场景，锚完好但 `ipfs cat` 全部失败，自愈链条断裂。

**不变量**：链锚（或 Relay 指针）引用的每个 CID，在「Relay 主机 + 同机 IPFS 全灭」时仍可取回。

**三层 pin 架构**（RPO 递减、成本递减、独立性递增）：

| 层 | 组件 | 触发 | 覆盖 | 链上痕迹 |
|----|------|------|------|----------|
| **热层** | `relay-pinner`（新，消费 Relay 持久化流）→ 自建 IPFS Cluster | 每次 `*_put` | 全部用户（**含 Relay-only 档**——他们没有链锚，路线只有这一条）、当前代 CID | 无 |
| **持久层** | `pallet-storage-service` Standard 档（3 副本 + 24h 巡检，基础设施已存在） | **周期性**（≈锚节奏，15min–1h） | 有锚用户 + 全员的 **Relay 当前指针 CID**（见下「可见性约束」） | relay-operator → CID |
| **灾备底** | Crust 存储单（自托管 W3Auth Pinning Service，标准 IPFS remote pin API） | 每日 | 同上（Relay 指针集合的当日快照） | Crust 链：operator seed → CID |

**可见性约束（关键）**：锚是密文，**运营者无法解出「锚内 CID」**——持久层与灾备底的 pin 对象只能是 **Relay 明文指针的当前值**（运营者从 `*_put` 天然可见）。在 hash-skip 语义下，Relay 指针与锚内 CID 在正常路径中收敛一致；客户端 §6.1 的写顺序（Relay 先落盘、链锚后发）保证指针不会落后于锚。因此持久层触发用**周期扫描 Relay 指针**，而非监听链上 `AnchorPublished` 事件（事件里也没有明文 CID 可用）。

**实现要点**：

- 热层 pinner 异步消费 Relay 持久化流，不阻塞写路径。**消费机制约束**：`relay-journal.ndjson` 在每次 snapshot 后被**截断**（`relay-persistence.mjs` `flushNow`），裸 tail 文件会在截断瞬间丢事件——pinner 必须采用「journal 带 seq 偏移 + 定期对 `relay-state.json` 全量对账」的双轨消费，或由 relay 进程内直接推送事件给 pinner。
- pin 前 `dag stat` 校验体积上限（pinner 拒收线 10MB/blob）；指针轮换用 `pin update` 换代，旧 CID 保留 2 代后延迟 unpin（防在途读与 LWW 竞态）。滥用面已由写鉴权 + 每账户 3 槽位 + `RELAY_RATE_LIMIT` 收敛，资源上界 = 用户数 × 3 × 体积上限。
- **体积上限两端对齐**：客户端写入侧对单个 sync blob 设上限 **8MB**（pinner 拒收线之下留余量），msg-archive 接近上限时收紧 `MaxPerConv` 裁剪或分页（§14.4）；否则超限 blob 会**静默失去多点保护**且客户端无感知。
- **隐私红线**：持久层与灾备底的 pin 请求方**必须是运营者账户**，绝不可用用户主账户——`request_pin_for_subject` 是 Signed extrinsic，用户签名 = 把 §9 否决的「`AccountId` → 明文 CID」从存储计费后门写回链上。Crust 同理（订单账户 = 自托管 W3Auth PS 的 operator seed）。Crust 链上公开「无主加密 blob 的 CID」属可接受泄漏，写入隐私披露。
- **部署红线**：Cluster 节点与 Relay 主机不同宿主机、建议不同供应商/区域；否则三层都是纸面冗余。
- **Crust 运维**：存储单约 6 个月有效期，启用预付池自动续期 + CRU 余额告警；`relay-sync-audit` 扩展三项检查——每个指针 CID 从 ≥2 个独立网关可取回、Cluster pin 状态、Crust 订单状态（W3Auth PS 状态库 + 抽样 `ipfs.io` 取回）。
- **节奏红线**：Crust 按 CID 下单，**禁止**对齐 `*_put` 或锚频率逐次下单（百万级订单/天不可接受）；每日快照节奏 + 仅对自上次下单后**发生变化**的 CID 下单即可。
- **媒体 CID 为独立决策**：msg-archive 引用的媒体若全死，恢复后归档为死链。NexChat **默认不做链上/运营全局 pin**（`VITE_IPFS_PIN_ENABLED=false`）；发送方本机 kubo pin + TTL（`senderMediaRetention.ts`），链上 Temporary Pin 仅 opt-in。成本敞口（体积无上界）与 sync blob 不同，不捆绑在本节范围内。

**CESS 评估结论（2026-06）**：否决。① 仍处 Pre-Mainnet Venus+ 测试网，官方明确不建议生产使用；② DeOSS 以自有 `fid` 寻址而非 IPFS CID，接入需改 manifest schema 与读路径，违背零侵入原则；③ 内置加密/纠删码对已加密 blob 无增益。其「指定矿工/地理坐标存储」能力对未来数据驻留合规有价值：**主网稳定运行 ≥6 个月后重评，定位限于 Layer B 备份 tarball 的异地目标**，不进入 C 层依赖链。

---

## 6. Client behavior / 客户端行为

### 6.1 写路径（push，tiered debounce）

```text
1. merge local + relay (+ chain if newer) → build IPFS blobs
2. ipfs add (仅变化的 blob) → SyncManifest
3. localStorage + relay index_put / contacts_put / msg_archive_put   // A — 先落盘
4. 若 manifest 相对上次已上链版本有变，且满足链 debounce / sign-out / manual：
     encrypt(SyncManifest, K_sync) → publish_sync_anchor (async)     // C
```

失败策略：

- **(3) Relay 失败**：local 仍有效；后台重试；**不阻塞发消息**  
- **(4) 链锚失败**：local + Relay 仍有效；入队重试；**不阻塞发消息**  
- **(4) 链锚成功、(3) 曾失败**：其他设备仍可从链 + IPFS 恢复（§6.3 写回 Relay）  
- **重试队列**：持久化于 localStorage（跨会话保留），指数退避（1min 起、上限 1h）；队列中仅保留**最新一份**待发 manifest（旧的被 LWW 取代，无需排队重放）

### 6.2 读路径（unlock / restore）

```text
sources = {
  local:  decrypt(localStorage fragments → manifest),
  relay:  relay fetch ×3 → manifest,
  chain:  sync_anchor(anchor_id) → decrypt(K_sync) → manifest,
}
for each field f in {index, contacts, archive}:        // 逐字段 LWW，
  eff[f] = argmax over sources of updated_at[f]        // 不做整份 manifest argmax
→ for each eff field: ipfs cat(cid) → decrypt blob → merge local
```

> 逐字段而非整份取最大：设备 A 的 contacts 较新、设备 B 的 archive 较新时，整份 argmax 会丢掉一方；字段级合并保证各取最新。

### 6.3 Relay 空库写回（C 的核心自愈）

```text
after restore from chain/local:
  for each field f in {index, contacts, archive}:        // 逐字段，与 §6.2 对齐
    if relay[f] missing or relay[f].updated_at < eff[f].updated_at:
      relay *_put(f, eff[f])
```

**触发**：`restoreOffchainData` 成功或 `partial` 后 **始终** 尝试写回（扩展 today 仅在 `ok|partial` 后 push 的逻辑，覆盖 `no_backup` + chain 有锚的情况）。

### 6.4 与 `offchainSync.ts` 的改动点

| 文件 | 变更 |
|------|------|
| **前置（§5.0）**：`src/keyvault/keyvault.ts` + `src/wallet/` | `keyVault.init` 换 `vault_master` 根 + `clear()`；本地库 / 三类 blob 自动迁移 |
| `src/sync/syncAnchor.ts`（新） | 锚密钥对派生、encrypt/decrypt manifest、`anchor_sig` 构造、chain publish/query |
| `src/store/offchainSync.ts` | 逐字段 LWW 读合并（§6.2）+ `no_backup` 时查链 |
| `src/keyvault/keyvault.ts` | `deriveSyncAnchorKeypair()` / `deriveSyncManifestKey(anchorId)` |

> 不再依赖 `inboxManager` 暴露 `inbox_id`：锚与投递 inbox 已解耦（§5.1）。

### 6.5 自愈边界（诚实声明：C 层恢复的是什么、不是什么）

「登录自愈」的准确含义：**加密数据快照自愈 + 会话密钥重建 + 投递层轮换**，不是全量恢复。边界矩阵：

| 数据 | 登录自愈？ | 说明 |
|------|-----------|------|
| 会话索引 / 通讯录 / 消息归档 | ✅ 快照级 | **RPO = 链锚长 debounce（15min–1h）**；manifest 指向的 blob 自身有短 debounce 上传滞后（二阶滞后）。锚发布后、Relay 失库前的增量丢失（除非他端设备在线补推） |
| 消息内媒体 CID | ⚠️ | 依赖 §5.8 媒体 pin 决策；未 pin / ephemeral 媒体取不回 |
| spent 防重放集合 | ❌ | §8.3 已声明不覆盖；空 Relay 上线即打开重放窗口，恢复后应提示 epoch bump 关窗 |
| RFC 9474 投递 inbox | ❌ 轮换重建 | RSA 私钥 + salt 仅存 localStorage，换机即丢；新设备生成新 `inbox_id` 重注册，联系人旧令牌全部作废、需重新获取盲签令牌，间隙内 sealed-sender 投递失败 |
| MLS 群 / 1:1 会话密钥 | ❌ **原理性不可** | TreeKEM 秘密前向保密，任何备份方案都不应恢复它（能恢复 = FS 被破坏）。新设备重新入群（external commit / 重邀请）、1:1 重握手；离线期发往旧 epoch 的消息永久不可解——**设计代价，非缺陷** |
| 瞬态邮箱（联系人请求 / 群邀请 pending） | ❌ | 空库即丢，依赖发起方重试 |
| 从未发布过锚的用户 | ❌ | 新用户未达长 debounce、链写持续失败、Relay-only 档——无锚可查（`no_backup` 分支） |

**灾后重建编排**（实现要求）：`restoreOffchainData` 成功（含从链锚恢复）后，客户端应自动触发统一序列——① 写回 Relay（§6.3）；② inbox 重建 + 重注册；③ 提示 epoch bump（关闭 spent 重放窗口）；④ 引导 MLS 重入群 / 重握手。这些不得停留为各模块的独立隐式行为。

**与 B 的定位关系**：C 是 B 失效后的**数据层降级救援**，不是等价替代——B 覆盖全员全量（含 spent、inbox 注册、邮箱），C 覆盖单用户三类指针快照。不得因 C 存在而降低 B 的运维标准（§8.2 同义）。

---

## 7. Product tiers / 产品档位

| 档位 | B 运维 | C 链上 | 适用 |
|------|--------|--------|------|
| **Standard**（默认） | ✅ | ✅ 长 debounce + sign-out（§11.2） | 换机双保险 |
| **Relay-only** | ✅ | ❌ | 极重隐私 |
| **Ops-only** | ✅ | ❌ + 无客户端链代码 | 私有部署 |

UI：设置 → 云同步 →「链上加密备份锚（推荐）」开关；默认开 Standard。

---

## 8. Parallel rules / 并行协同

### 8.1 单一 LWW 真相源

所有层对 **SyncManifest 整体** `updated_at` 以及 **各字段** `updated_at` 做 LWW；**客户端** 是仲裁者，Relay/链不互相同步。

### 8.2 运维 restore 与链锚冲突

- **生产切机**：以 **B restore 后的 Relay** 为起点；用户登录后若 chain 更新，正常 LWW 覆盖。  
- **空库应急**：仅 C + 用户写回；B 仍可用于 **事后** 一次性 restore spent。

### 8.3 spent / mailbox

| 状态 | B | C |
|------|---|---|
| spent 防重放 | ✅ restore | ❌ 不覆盖；依赖 inbox epoch bump 或接受窗口 |
| MLS/contact mailbox | ✅ restore | 在线重建 |

**ADR 声明**：C **不**承诺 spent 迁移；生产换机 **必须** 走 B。

---

## 9. Rejected alternatives / 否决方案

| 方案 | 否决理由 |
|------|----------|
| `AccountId → 明文 CID` | SS58+CID 公开索引；P2 精神冲突 |
| `InboxId → 密文`（本 ADR v1 草案） | **新设备无法重算 `inbox_id`**（`H(随机 RSA 公钥 ‖ 随机 salt)`，仅存 localStorage）→ 主用例「换机 + 空 Relay」不成立；controller=主账户与 `pallet-chat-inbox` 的 throwaway 不变量冲突；跨 pallet 复用键会把投递信箱关联到 sync 活动 |
| `AccountId → 密文` | v1 隐私与 AnchorId 等价（extrinsic 付费关联已泄漏），但键永久绑定账户、无法升级到匿名提交；AnchorId + 锚签名以同等成本保留 v2 断链路径 |
| 客户端多点直写（`ipfs add` ×N 端点） | 公开可写端点必然被滥用；端点列表烧进构建配置；客户端双倍上传延迟；存储付费无解。多点存活由 §5.8 服务端三层 pin 承担 |
| CESS 作为 C 层存储 | 截至 2026-06 仍为 Pre-Mainnet 测试网（官方不建议生产）；`fid` 寻址与 CID 体系冲突需侵入式改造（详见 §5.8，主网后限 Layer B 重评） |
| 用户主账户发起链上 / Crust pin | `request_pin_for_subject` 为 Signed extrinsic，用户签名 = 「`AccountId` → 明文 CID」从存储计费后门回流上链（§5.8 隐私红线） |
| 仅 C、无 B | spent/离线全员/运维失误无兜底 |
| 仅 B、无 C | 新设备+空 Relay 无 rsync 时无用户自愈 |
| 每次 `*_put` 三笔链上写 | 成本与链负载不可接受 |
| 链上存 SyncManifest 明文 | 泄漏 CID 与备份结构 |

---

## 10. Implementation plan / 落地清单

| Phase | 交付 | Owner |
|-------|------|-------|
| **P0** | **§5.0 前置**：`vault_master` 换根（弃用地址种子）+ 本地库 / blob 自动迁移 + 派生固定测试向量 | NexChat |
| **P0** | B：`relay-backup-to-ipfs.sh` / `relay-restore-from-ipfs.sh`（待实现）+ timer 生产化 + runbook + `latest-backup.json` 异地存放与监控 | Ops |
| **P0** | **§5.8 热层**：`relay-pinner`（seq+对账双轨消费持久化流 → 异机 IPFS Cluster，体积上限 + pin update 换代 + audit 取回率检查） | Relay + Ops |
| **P1** | `pallet-chat-sync`（AnchorId + 锚签名校验 + `clear_sync_anchor` + 押金）+ runtime + `chat_syncAnchor` RPC + 单测 | Chain |
| **P1** | 客户端 `syncAnchor` + 逐字段三方 merge + 空 Relay 写回 + §6.5 灾后重建编排 | NexChat |
| **P1** | **§5.8 持久层**：锚发布挂钩 `pallet-storage-service` Standard pin（运营者账户请求） | Chain + Relay |
| **P2** | 设置档位 Relay-only / Standard；隐私文案（含 §5.8 Crust CID 披露） | NexChat |
| **P2** | **§5.8 灾备底**：自托管 W3Auth Pinning Service + Crust 每日下单 + 预付池续期 + CRU 余额告警 | Ops |
| **P2** | E2E：新设备仅凭助记词 + 空 Relay 恢复；B restore + 链较新 LWW；**断开 Relay 主机及其同机 IPFS 后锚内 CID 仍可取回** | QA |
| **P3** | v2 付费方断链（proxy / 一次性账户 / 赞助费率，§11.1） | Chain + NexChat |

### 10.1 文档交叉引用

- `CHAT_P2_SESSION_ANCHOR_DESIGN.md` — 会话关系仍不上链；§2.1 明文指针 → 本 ADR C 层  
- `CHAT_OFFCHAIN_DELIVERY_DESIGN.md` — inbox_id 与 controller（背景参考；v2 起锚与 inbox 解耦）  
- `nexchat/docs/RELAY_PERSISTENCE.md` — Layer A + B  
- `CHAT_FRONTEND_PLAN.md` — KeyVault KDF 路径补 `vault-master/v1`、`sync-anchor-key/v1`、`sync-manifest/v1`

---

## 11. Review decisions / 评审决议（建议默认值）

CN: **11.1 为已定决议**（v2 键模型重做的直接推论）；**11.2–11.4、11.6** 为进入实现前的 **建议默认**（评审可推翻）；**11.5** 仍留链上经济参数给 runtime 配置。

### 11.1 授权与付费分离（原「Inbox controller / v1 签名者」，v2 重做）

**决议（v2）**：

- **授权** = 锚密钥 Ed25519 签名（`anchor_sig`，§5.5），与 extrinsic origin 无关；不存在 controller 概念，也不查 `pallet-chat-inbox`。  
- **付费**（手续费 + 押金）= extrinsic origin；**v1 = 主聊天 `AccountId`**，链上付费关联如实披露（§5.7）。  
- **v2 断链选项**：origin 换 proxy / 一次性账户 / 赞助费率即可移除付费关联；存储键与授权逻辑零迁移。

原 v1 草案「controller = 主账户 vs throwaway」之争随键模型重做而消解：锚不注册于 `pallet-chat-inbox`，投递 inbox 的 throwaway 不变量不受影响。

### 11.2 Debounce：客户端 vs `MinBlocksBetweenPublish`（原 open Q2）

**建议默认**：

| 层 | 机制 |
|----|------|
| **客户端（主）** | 生产链锚 debounce **≥15min**，另 **sign-out / 手动备份** 必触发；见 §14.3 右列 |
| **链上（硬顶）** | `MinBlocksBetweenPublish = 100` blocks（~10min @ 6s），**spam 防护** |
| **生效规则** | 两者 **取较严者**：未到客户端长 debounce **且** 未过链上块高间隔 → 不发 extrinsic |

**不**在 runtime 写入 wall-clock debounce（仅客户端配置 + 链上块高硬顶）。

### 11.3 Relay-only 默认档位（原 open Q4）

**建议默认**：

- 全球构建：**Standard**（B 运维 + C 链锚，C 按 §11.2 长 debounce）。  
- 高合规 / 欧盟等区域构建：**默认 Relay-only**（C opt-in，UI 明示链上锚活跃度 metadata）。  
- 实现：`VITE_SYNC_ANCHOR_DEFAULT=standard|relay_only` 或等价构建 flag。

### 11.4 Debounce 配置存放位置（原 open Q5）

**建议默认**：Relay / 链 **debounce 与 hash-skip 逻辑均在客户端**（`OffchainSyncCoordinator`）；链上仅 **`MinBlocksBetweenPublish`** 硬顶（`MaxAnchorsPerBlock` 已于 §5.4 移除）。规模化分档见 §14.3，**不**增加 runtime 常量。

### 11.5 仍待 runtime 拍板（原 open Q3）

**Storage deposit**：`SyncAnchors` 条目 **固定小额 reserved**（`AnchorDeposit`，不随 `MaxAnchorLen` 线性涨），首次 publish 时从付费 origin 收取并记 `depositor`，`clear_sync_anchor` 退还（§5.5）；具体数值在 `pallet-chat-sync` benchmark + 经济评审后填入 `Config`，v1 可先用与 `pallet-chat-inbox` 同量级押金。

### 11.6 键模型（补充，v2 更新）

- **链（C）**：`AnchorId`（§5.1 / §5.3）。  
- **Relay（A）**：仍为 **`AccountId` SS58**（`register_account`）。  
- 两个键均可由助记词单独重算（恢复闭环成立），**无需对齐**；v1 草案的「P3 Relay 键 inbox_id 化」不再因对齐而必要，仅当投递架构自身需要时再评估。

### 11.7 pallet 深审拍板（v2.3：重放骚扰 / clear 复活 / 治理逃生门）

2026-06-12 pallet 深度审计三项决议（实现已落地，见 §5.4 / §5.5）：

1. **等值重发幂等 no-op（限频骚扰修复）**：publish 签名不绑定 origin（付费/授权分离的设计代价），公开的 (payload, sig) 任何人可重发。修复前重发会重置 `last_publish_block`，可被用于压制持有者的真实更新；修复后内容未变的重发不写状态、不动限频时钟。  
2. **clear 墓碑水位（防历史复活）**：选择 `ClearedAt` 永久水位而非「文档化接受」——主网前无迁移成本，40B/锚 的存储残留换取「主动删除不可被第三方撤销」的强语义（删除即承诺）。  
3. **`ForceOrigin` 逃生门**：与 `pallet-chat-inbox` 的 `force_deregister_inbox` 对齐（runtime 同用 `RootOrTechnicalMajority`）。动机：锚密钥派生自助记词、可能永久丢失，无逃生门则记录+押金永久滞留，且链上密文若涉滥用内容无任何治理处置面。force-clear 退押金 + 写墓碑 + 独立事件；**不构成审查能力**——持有者可随时以更新清单重建。

---

## 12. Acceptance criteria / 验收标准

- [ ] 前置（§5.0）：`vault_master` 换根完成；K_index / K_contacts / K_archive / K_sync 均不可由公开地址派生（单测 + 迁移幂等）  
- [ ] B：从 `latest-backup.json` restore 后，`admin_stats` 指针/spent 计数与备份前一致  
- [ ] C：**新设备仅凭助记词**（无 localStorage）+ 新 Relay **空库**，重算 `anchor_id` → 链锚 + IPFS 恢复 conv/contacts/archive，且 Relay 被写回  
- [ ] Blob 可用性（§5.8）：**断开 Relay 主机及其同机 IPFS** 后，锚内 CID 仍可经独立网关取回；`relay-sync-audit` 取回率 / pin 状态 / Crust 订单三项检查通过  
- [ ] 灾后重建（§6.5）：从链锚恢复后自动触发 Relay 写回 + inbox 重注册 + epoch bump 提示  
- [ ] 授权：非锚密钥持有者无法 publish/clear（含首次发布 mempool 抢跑场景单测）；`updated_at` 超前 `MaxClockSkew` 被拒  
- [ ] 并行：B restore 后，链上较新 manifest 可在下次登录按字段 LWW 覆盖 Relay  
- [ ] 隐私：链上 storage 无明文 CID、无 `AccountId → anchor` 存储映射（v1 付费关联已在 §5.7 披露）  
- [ ] Relay-only 档：无 `publish_sync_anchor` extrinsic，功能仅依赖 A+B  
- [ ] Scale（§14）：解锁不等待 archive；链锚失败不阻塞发消息；manifest 不变时不发 extrinsic

---

## 13. Amendment to CHAT_P2 §2.1（建议文案）

> **原**：指针不上链。  
> **增**：云同步 **明文指针不上链**；可选 **账户派生 Encrypted Sync Anchor**（本 ADR §5，键为助记词可重算的不透明 `AnchorId`）作为用户可移植备份锚，**不**存储会话关系、明文 CID 或投递 inbox 关联。运维整库备份（本 ADR §4）为换机主路径。

---

## 14. Scale / UX at high user volume / 大规模用户与体验

CN: 用户规模上升时，**热路径（聊天/投递）与冷路径（云同步/链锚/备份）必须分离**。Layer A 决定「能不能聊」；B/C 与 IPFS 必须 **后台化、合并写、可降级**，不得阻塞解锁与发消息。

EN: As user volume grows, **keep the hot path (chat/delivery) separate from the cold path (cloud sync / chain anchor / backup)**. Layer A determines whether chat works; B/C and IPFS must be **background, batched, and degradable** — never blocking unlock or send.

### 14.1 Two user paths / 两条用户路径

| Path | User-facing | Latency budget | Scales poorly when |
|------|-------------|----------------|---------------------|
| **Hot / 热** | Send/receive, MLS, RFC 9474 delivery | ms–100ms | WS connections, fan-out, spent lookups, sync fsync on same process |
| **Cold / 冷** | Unlock restore, cloud backup, EISA | seconds OK | IPFS upload, triple `*_put`, extrinsics, full `data/` tar |

**Invariant / 不变量**：cold path **must not block** hot path.

```text
Unlock → enter chat immediately (local + Relay hot state)
       → background: IPFS / Relay pointers / chain anchor (retry, degrade)
```

### 14.2 Layer A — Relay hot service / Relay 热服务

**Current bottleneck / 现状瓶颈** (single Node + file WAL):

- Per-connection memory; pointer `*_put` **journal fsync** → disk IOPS under sync load  
- Single-process WS ceiling (~10k connections → need horizontal scale)

**Evolution / 演进** (does not change A+B+C semantics):

```text
                 ┌─ Relay-Delivery (light state) ─┐  MLS fan-out, mailboxes
  LB ─ WebSocket ─┤                                 │
                 └─ Relay-Sync-KV (shared store) ──┘  pointers, inbox, spent
                            │
                     Redis / Postgres
                     (replaces per-host data/ files at scale)
```

| Stage | Action | Rough scale |
|-------|--------|-------------|
| P1 | Single Relay + metrics (connections, journal lines, fsync latency) | < ~10k DAU |
| P2 | **Split delivery vs sync KV** (process or port) | Chat not blocked by sync fsync |
| P3 | **Shared storage** + multiple Relay instances + sticky LB | Horizontal scale |
| P4 | Multi-region Relay + regional IPFS gateway | Global latency |

**Hot-path protections / 热路径保护** (implement regardless of C):

- Do not run heavy sync fsync on the same critical path as message fan-out (split or batch).  
- Keep `inboxById` O(1); cap + epoch-prune `spent` (`RELAY_SPENT_CAP`).  
- Sticky sessions by account where possible.  
- Existing `RELAY_RATE_LIMIT` / `RELAY_MAX_MSG_BYTES` — tune per tier at scale.

### 14.3 Layer C — EISA write/read at scale / 链锚规模化

**Must not happen / 禁止**:

- One extrinsic per conv-index edit × millions of users → chain congestion, fee spikes, blocked UX.  
- Full chain + triple IPFS pull on every unlock → slow cold start.

**Write policy / 写策略** (preserve UX):

| Policy | Detail |
|--------|--------|
| **One extrinsic per debounce window** | After IPFS blobs settle, single `publish_sync_anchor` (§5) |
| **Split debounce** | Relay pointer push: **3–15s**; chain anchor: **15min–1h**, or on **background / sign-out / manual backup** |
| **On-chain rate limit** | `MinBlocksBetweenPublish` (§5.4) + client retry queue |
| **Skip if unchanged** | Hash/compare SyncManifest; no extrinsic if identical |
| **Relay-only tier** | No chain writes (§7) |

Chain anchor failure **must not** block sending messages; it only affects portable recovery.

**Read policy / 读策略** (fast → slow):

```text
1. localStorage pointers          (sync)
2. Relay fetch                    (~100ms; usually enough)
3. chain sync_anchor              only if Relay empty or stale vs local
4. IPFS blobs                     lazy: index → contacts → archive last
```

- **First screen**: restore conv-index → show conversation list; **archive in background**.  
- Cache chain RPC result client-side (`updated_at`); avoid repeat queries per session.

**Suggested debounce defaults at scale / 规模化建议默认值**:

| Traffic | Relay `*_put` debounce | Chain `publish_sync_anchor` |
|---------|------------------------|-------------------------------|
| Dev | 1.5s | 15min or manual |
| Production < 100k DAU | 3–5s | 15min + sign-out |
| Production 100k+ DAU | 5–15s (unified coordinator) | 1h / sign-out / manual; optional disable Standard default |

Align **`MinBlocksBetweenPublish`** with §11.2：客户端长 debounce 为主，链上块高为硬顶（100 blocks ≈ 10min @ 6s block，取较严者）。

### 14.4 IPFS blobs / IPFS

| Risk | Mitigation |
|------|------------|
| Re-upload unchanged content | Skip `add` if local hash → CID unchanged |
| Large msg-archive | Keep `MaxPerConv` cap; future: incremental / per-conv blob pages |
| Slow gateway | Regional gateway + CDN (`VITE_IPFS_GATEWAY_URL`) |
| Unpinned content | §5.8 三层 pin（relay-pinner 热层 / storage-service Standard / Crust 灾备底）；媒体 Temporary for cold archive |
| Slow unlock | Parallel `cat`; prioritize index over archive |

### 14.5 Layer B — Ops backup at scale / 运维备份规模化

| Small deploy | Large deploy |
|--------------|--------------|
| `relay-backup-to-ipfs.sh` timer; optional brief stop | **No stop-the-world tar** on every tick |
| Full `data/` tar | **Shared DB PITR / replica snapshot** when Relay uses Redis/Postgres |
| 15min–daily | Incremental WAL archive + daily full |

B remains **SRE-only**; users unaffected. At scale, migrate B from file tar to **DB backup** when Layer A moves off file WAL.

### 14.6 Client — unified cold pipeline / 客户端统一冷路径

Today three sync modules each debounce ~1.5s → up to **3× IPFS + 3× relay put + 1× chain**.

**Target / 目标**: one `OffchainSyncCoordinator`:

```text
debounce (tiered)
  → merge local
  → upload only changed IPFS blobs
  → relay puts (or future sync_batch frame)
  → publish_sync_anchor (longer debounce, hash gate)
```

**接口草案（P1 实现基线，细化在实现 PR 中完成）**：

```ts
interface OffchainSyncCoordinator {
  markDirty(field: "index" | "contacts" | "archive"): void;  // 各模块只调这个
  flushRelay(): Promise<void>;     // 短窗口到期：blob 上传 + *_put（仅变化字段）
  flushChain(): Promise<void>;     // 长窗口/sign-out/manual：canonical manifest
                                   //   → hash gate → publish_sync_anchor → 重试队列
  restore(): Promise<RestoreResult>; // §6.2 逐字段三源合并 + §6.3 写回 + §6.5 编排
}
```

Extend unlock flow (§6.3): on `no_backup`, **background** query chain + warm empty Relay — do not block UI.

### 14.7 Degradation matrix / 降级矩阵

| Failure | User can still | Degrade |
|---------|----------------|---------|
| Chain congested | Chat | Skip anchor; Relay + local only |
| Relay slow | Chat (local queue) | Retry sync later |
| IPFS slow | Chat (local cache) | Partial restore + banner |
| All cold paths down | Read local threads | Read-only / offline mode |

**Product rule / 产品规则**: **chat usable > cloud sync complete**.

### 14.8 Scale checklist by phase / 分阶段清单

**~10k DAU**

- [ ] Relay metrics + alert on journal growth / fsync p99  
- [ ] Client: lazy archive on unlock; chain anchor debounce ≥ 15min  
- [ ] B: backup timer + restore drill  

**~100k DAU**

- [ ] Relay delivery vs sync split OR shared Redis/Postgres KV  
- [ ] Regional IPFS gateway  
- [ ] Chain: manifest hash skip + `MinBlocksBetweenPublish` enforced  
- [ ] Unified offchain debounce coordinator  

**~1M+ DAU**

- [ ] Multi-instance Relay + global KV store  
- [ ] Chain anchor primarily **sign-out / daily / manual**; Standard opt-in  
- [ ] Sharded / incremental msg-archive blobs  
- [ ] B: DB PITR, not file tar  

### 14.9 Relation to A+B+C / 与三层并行关系

| Layer | At scale |
|-------|----------|
| **A Relay** | **Must** evolve to cluster + shared storage — UX-critical |
| **B Backup** | Incremental / DB; no stop-the-world |
| **C EISA** | **Lower write frequency**, conditional read; recovery without hot-path cost |

Parallel architecture **unchanged**: B for ops cutover; C for empty-Relay self-heal; scale by **tightening C cadence and scaling A**, not by adding plain `AccountId → CID` on-chain.

---

## 15. Revision history / 修订记录

| 版本 | 日期 | 变更 |
|------|------|------|
| v1 | 2026-06-10 | 初稿：`InboxId` 键控 + inbox controller 授权 |
| v2 | 2026-06-10 | 评审修订：① 链上键改为助记词可重算的 **`AnchorId`**（修复 v1 致命缺陷：`inbox_id = H(随机 RSA 公钥 ‖ 随机 salt)` 仅存 localStorage，新设备无法重算，C 层主用例不成立）；② 授权改为**锚密钥 Ed25519 签名**，付费与授权分离，消除首发抢跑与「controller 隐私」之争；③ 新增 **§5.0 `vault_master` 硬前置**（现 keyVault 基钥 = SHA-256(公开地址)，anchor_id / K_sync 否则全网可算）；④ 补 `clear_sync_anchor` + 押金退还、`updated_at` 上界 `MaxClockSkew`（防自锁）；⑤ 移除 `MaxAnchorsPerBlock`；⑥ §6.2 改**逐字段 LWW** 合并；⑦ §4 补 GPG 口令管理、备份 CID 异地存放、spent 重放窗口；⑧ §9 补 `InboxId → 密文`、`AccountId → 密文` 否决分析 |
| v2.1 | 2026-06-10 | 自愈承诺收敛：① 新增 **§5.8 blob 可用性硬前置**——三层 pin（relay-pinner 热层 / `pallet-storage-service` Standard 持久层 / Crust 灾备底），修复「锚指向的 CID 与 Relay 同机同灭」的相关性故障；明确 pin 请求方必须为运营者账户的隐私红线与 Crust 节奏红线；CESS 评估否决（测试网 + fid 寻址），主网后限 Layer B 重评；② 新增 **§6.5 自愈边界矩阵**——TL;DR「用户登录自愈」改为「数据层登录自愈」，明确 spent / 投递 inbox / MLS 会话密钥 / 瞬态邮箱 / 无锚用户不在覆盖内（MLS 为前向保密的设计代价），并要求灾后重建统一编排；③ §9 / §10 / §12 / §14.4 同步更新 |
| v2.3 | 2026-06-12 | pallet 深审硬化（§11.7）：① **等值重发幂等 no-op**——内容未变的 publish 重发不写状态、不重置限频时钟，封堵「重放公开 (payload, sig) 压制持有者更新」的限频骚扰（§5.5 规则 4b）；② **clear 墓碑水位 `ClearedAt`**——向已 clear 锚 publish 须严格大于水位，历史载荷重放不能复活主动删除的锚（§5.4 / §5.5 规则 4c）；③ **`force_clear_sync_anchor`（ForceOrigin）**治理逃生门——退押金 + 写墓碑 + 独立事件，与 inbox `force_deregister` 同口径；④ 修正 §5.5 抢跑分析：原封载荷抢跑会落账但仅替持有者垫付押金（配套命名单测）；⑤ pallet 加 `storage_version(1)` |
| v2.2 | 2026-06-10 | 开发就绪审计修复：① **修正 §5.8 可见性悖论**——锚为密文、运营者不可见「锚内 CID」，持久层/灾备底 pin 对象改为 Relay 明文指针集合、周期触发（非链事件）；② **pinner 消费约束**——journal 被 snapshot 截断，须 seq 偏移 + state 全量对账双轨消费；③ **三项字节级合同**：`anchor_sig` payload 编码（裸拼接 + u64 LE + 双端测试向量）、SyncManifest canonical JSON（键排序/无空白）、`sr25519_secret` 统一取自 `KeyringPair.encodePkcs8()` 解包（覆盖 mnemonic/JSON 导入/dev URI 全部来源）；④ pallet 外观面补 Events / Errors / Config 项；边界语义：`updated_at ==` 允许、`depositor` 后续 publish 不变、clear 不存在锚返回 `AnchorNotFound`；⑤ blob 体积上限两端对齐（客户端 8MB / pinner 10MB）；blob 版本字节回退解析规则；⑥ 链锚重试队列语义（localStorage + 指数退避 + 仅留最新）；⑦ `OffchainSyncCoordinator` 接口草案（§14.6）；⑧ 清理矛盾：§11.4 残留 `MaxAnchorsPerBlock`、§11.3 inbox→锚活跃度、§6.3 改逐字段写回、§11 引言 11.1 升为已定决议 |

---

*End of ADR*
