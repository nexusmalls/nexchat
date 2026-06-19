# Chat 1:1 私聊 · X3DH + Double Ratchet · 开发文档 / Dev Design

> 状态：**实现规范 · v2**（v1 设计草案的开放问题已冻结为决策，见 §16；新增 §17–§21 字节级实现规范）
> 定位：**1:1 私聊的替代密码学栈**——以去中心化 **X3DH 建会话 + Double Ratchet 持续加密** 取代当前
> 「1:1 = pairwise OpenMLS（Wire 多 leaf）」路线；**群聊（≥3 人）继续 OpenMLS**，两者**严格解耦、独立模块**。
> 本文是开发落地文档：给出模块边界、链上/链下职责、协议流程、存储隔离、2↔3 人切换、多设备、测试与分期，
> 并在 §16–§21 给出可直接开工的密钥派生 ADR、X3DH/DR 序列化、relay 帧 schema、OPK 生命周期、模式协商。
>
> 仓内已核实事实（决策依据）：账户为 **sr25519**（SS58 273/42，不能直接做 X25519 DH）；`nexchat/src/mls/devicePeerKey.ts`
> 已有「设备专属 ECDH 钥（不从 `vault_master` 派生）+ 账户钥签名背书 + WebCrypto HKDF-SHA256」先例；
> `nexchat/src/wallet/vaultMaster.ts` 定义 `vault_master = HKDF-SHA256(sr25519_secret(64B), "nexchat/vault-master/v1", ss58)`
> 且有冻结测试向量；`pallet-crypto-common::EncryptionMethod` 已枚举 AES-256-GCM / ChaCha20 / XChaCha20-Poly1305；
> relay 帧 `RelayFrame{ convId, senderRef, ciphertextB64, delivery?, routeTo?, echoSelf? }` 与控制面 `ControlMsg`
> 见 `nexchat/src/relay/relayClient.ts`。
>
> 关联（仓内现状，本方案的对照基线）：
> - `pallets/chat/group/src/lib.rs`（群 MLS DS/AS：`commit(expected_epoch)` / `TwoMemberGroupForbidden`）
> - `pallets/chat/CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md`（**当前** 1:1 = pairwise MLS Wire）
> - `pallets/chat/CHAT_GROUP_WIREIFY_DESIGN.md`（群多设备 Wire 化）
> - `pallets/chat/permission`（`CapabilityEpoch` 账户级撤销）/ `pallets/chat/inbox`（盲化一次性投递令牌）
> - `pallets/chat/sync`（EISA：`K_archive` / `K_contacts` / `K_index` 助记词自愈，与会话密钥正交）
> - `nexchat/relay-rs`（唯一 relay 实现：`d:` 路由键、`s:<account>` 自通道、backlog 重投）
>
> 关联（标准/外部）：Signal X3DH 规范、Double Ratchet 规范；MLS = RFC 9420（群侧，本文不改）。

---

## 0. TL;DR

CN：1:1 私聊改用 **Signal 式 X3DH + Double Ratchet**：链上只托管**身份预密钥**（IK/SPK/OPK Merkle 根），
握手与棘轮全在客户端、密文经 relay 点对点投递、**不建任何链上群**。群聊（≥3 人）**完全不变**，仍走 OpenMLS。
两套密码学**编译期隔离**：独立引擎、独立存储、单向依赖共用底座（身份 + 传输 + 归档）。2↔3 人切换由
**orchestrator** 显式编排——**不迁移密钥**，只在共用身份之上各自重建会话，历史正文凭 `K_archive` 自愈。

EN: 1:1 DMs switch to **Signal-style X3DH + Double Ratchet**. The chain custodies only **identity prekeys**
(IK / SPK / OPK Merkle root); handshake & ratchet run fully client-side; ciphertext goes peer-to-peer over the
relay; **no on-chain group is ever created**. Group chat (3+) is **unchanged** (OpenMLS). The two crypto stacks
are **compile-time isolated**: separate engines, separate stores, one-way deps on a shared base (identity +
transport + archive). The 2↔3 transition is orchestrated explicitly — **no key migration**; sessions are rebuilt
on top of the shared identity, and readable history self-heals via `K_archive`.

---

## 1. 目标与非目标 / Goals & Non-goals

### 1.1 目标
- 1:1 私聊使用 **去中心化 X3DH**（依托链 + relay 分发预密钥，无中心化密钥服务器）。
- 会话加密使用 **Double Ratchet**：每条消息独立派生密钥，前向保密（PFS）+ 泄露后自愈（PCS / break-in recovery）。
- 与群聊 OpenMLS **严格解耦**：DR 与 MLS 之间**零共享密码学状态**，靠架构强制而非纪律维持。
- **共用一套身份**（同一 Substrate Ed25519 账户钥派生）与**一套传输**（relay，可选 libp2p），用户只维护一套密钥。
- 2↔3 人切换对用户在 UI 层无感；密钥层为**全新会话**，历史走独立归档层。

### 1.2 非目标
- **不改群聊**：`pallet-chat-group` 与群侧 nexchat 不动（仍 OpenMLS DS/AS）。
- **不做密钥层无缝迁移**：DR 与 MLS 密钥不可互通；切换=旧会话归档 + 新会话建立。
- **不隐藏「在线节点拓扑」之外的元数据**：relay 仍见化名路由键（沿用现有 sealed/inbox 边界）。
- **不在链上存私聊内容/棘轮态**：链只存身份预密钥锚点。

### 1.3 与当前 MLS 1:1 路线的关系（务必读）
- 当前 of record：1:1 = **pairwise OpenMLS Wire**（`CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md`），靠 relay
  `commit_slot` CAS 防分叉、设备区分 leaf 做多设备 PCS。
- 本方案是**替代栈**：采用后，1:1 侧的 MLS 引擎、CD 选举、relay commit-slot CAS、设备 leaf 绑定等**整体退役**
  （`d:` 会话不再有 epoch / Commit 概念）；群侧零影响。
- **二选一，不并存**：同一账户的同一对 1:1 会话不得同时存在 DR 与 MLS 两种状态（迁移期以 flag + 版本号收口，§12）。

---

## 2. 分层与解耦边界 / Layering & decoupling boundary

「严格解耦」**精确限定在会话引擎 + 本地存储**；身份层与传输层**共用**（否则是过度拆分、逼用户管两套钥）。

```text
                       ┌───────────────────────────────────────┐
                       │            共用底座 / shared base        │
                       │  identity（账户钥 + X3DH 预密钥 + MLS KP） │
                       │  transport（relay / libp2p，前缀分流）     │
                       │  archive（EISA K_archive 历史，密钥正交）  │
                       └───────────────┬───────────────┬─────────┘
                                       │ 单向依赖        │ 单向依赖
                       ┌───────────────▼──────┐  ┌──────▼───────────────┐
                       │  crypto-dr（1:1 私聊） │  │  crypto-mls（群聊 3+） │
                       │  X3DH + Double Ratchet │  │  OpenMLS / TreeKEM    │
                       │  DR 会话库（独立）      │  │  MLS 会话树库（独立）   │
                       └───────────┬──────────┘  └──────────┬───────────┘
                                   │                         │
                                   └──────► orchestrator ◄────┘
                                     （2↔3 切换，仅调用建/销毁，不读密钥态）
```

| 层 | 解耦/共用 | 硬规则 |
|----|-----------|--------|
| 会话引擎 | **严格解耦** | `crypto-dr` 与 `crypto-mls` **互不 import**（编译期禁止） |
| 本地存储 | **物理隔离** | 独立数据库表/文件夹/persistKey；不共用内存变量 |
| 身份层 | **共用** | 同一 Ed25519 账户钥为根；身份层不感知会话类型，单向被依赖 |
| 传输层 | **共用** | 一套 relay，协议前缀分流：`/msg/private` ↔ `d:`、`/msg/group` ↔ `g:` |
| 归档层 | **共用** | `K_archive` 历史正文，独立于 DR/MLS 密钥 |
| orchestrator | 依赖两引擎 | 仅调用「创建/销毁会话」入口，**不得**读取/搬运任何密钥态 |

**核心 invariant（必须在 CI / lint 层强制）**：
1. DR 与 MLS **不共享** 任何 chain key / nonce / 根密钥 / 棘轮变量。
2. 两栈唯一接触点是身份层，且为**单向只读**派生。
3. orchestrator 切换会话**绝不迁移密钥**。

---

## 3. 模块清单与依赖方向 / Modules & dependency direction

建议的代码边界（客户端 = nexchat/TS + wasm；链上 = pallet）。**箭头方向即唯一允许的依赖方向**。

```text
chain:
  pallet-msg-identity        # 新增：X3DH 预密钥锚点（IK / SPK+sig / OPK Merkle 根）
  pallet-chat-group          # 不变（群 MLS DS/AS）
  pallet-chat-permission     # 复用（CapabilityEpoch：可作 X3DH 会话撤销锚）
  pallet-chat-inbox          # 复用（盲化一次性投递令牌：私聊密文投递鉴权）
  pallet-chat-sync           # 复用（EISA K_archive 历史归档）

client (nexchat):
  identity/                  # 共用：账户钥 → X3DH bundle / MLS credential（单向被依赖）
  transport/                 # 共用：relay 客户端 + 前缀分流（d: / g:）
  archive/                   # 共用：K_archive 读写
  crypto-dr/                 # 新增独立模块（仅依赖 identity + transport + archive）
    x3dh-handshake
    double-ratchet
    dr-session-store
  crypto-mls/                # 现有（群），1:1 部分退役；仅依赖 identity + transport + archive
  orchestrator/              # 2↔3 切换状态机；依赖 crypto-dr + crypto-mls 的公开入口
```

依赖硬规则（建议用 `eslint` import 边界 / Rust crate 边界强制）：
- `crypto-dr` 不得 import `crypto-mls`，反之亦然。
- 两 crypto 模块只依赖 `identity` / `transport` / `archive` 的**公开接口**。
- `orchestrator` 只能调用两模块导出的 `create*` / `destroy*` / `isActive`，**禁止**触达内部 `*Store` / 密钥结构。

---

## 4. 身份层（共用底座）/ Identity layer (shared)

### 4.1 一套身份服两套协议（决策：DH 钥与签名根分离）
**关键约束**：账户钥是 **sr25519（Ristretto）**，**无法直接做 X25519 DH**。因此 X3DH 的 DH 密钥**不复用账户钥**，
而是独立的 **X25519（Curve25519）** 密钥，由账户 sr25519 钥**签名背书**（与 `devicePeerKey.ts` 的「设备 ECDH 钥 +
账户签名背书」同一模式）。
- **签名根 / 信任锚**：账户 sr25519 钥。链上 `pallet-msg-identity` 的写入用**签名 origin**，故链天然知道 `AccountId`
  即背书者，无需另存账户公钥。
- **X3DH 三件套（均为 X25519，按设备）**：
  - `IK_dev`（设备身份 DH 钥，长期）；
  - `SPK_dev`（中期签名预密钥，账户钥签 `SPK_dev_pub`）；
  - `OPK_dev[i]`（一次性预密钥集合，只上 Merkle 根）。
- **MLS 需要**（群侧不变）：`KeyPackage` + E2EI 设备 leaf 绑定。
- **桥接而非隔离**：X3DH 与 MLS **同一签名根（账户钥）**，但 DH 钥、KeyPackage **字段独立、互不引用会话**。
- **为何按设备**：多设备每台需独立棘轮（§8 nonce 红线），故身份预密钥**按 `(AccountId, DeviceId)`** 键控，
  与群侧「每设备 leaf」对称。

### 4.2 密钥派生 ADR / Key-derivation ADR（冻结）
派生规则区分「可凭助记词重算的长期/中期钥」与「必须随机、用后即删的一次性钥」：

| 密钥 | 曲线 | 派生 | 可助记词重算 | 落库 |
|------|------|------|--------------|------|
| `IK_dev` 私钥 | X25519 | **设备专属随机**（仿 `devicePeerKey`，**不**从 `vault_master` 派生） | 否（设备丢失即换设备身份） | 身份库（设备本机） |
| `SPK_dev` 私钥 | X25519 | `HKDF-SHA256(vault_master, salt="nexchat/x3dh/spk/v1", info=ss58‖device‖epoch_le)` | 是 | 身份库 |
| `OPK_dev[i]` 私钥 | X25519 | **CSPRNG 随机**（前向保密要求；**不可**派生/不可重算） | 否 | 身份库，**用后即删** |
| 背书签名 | sr25519 | 账户钥对 `IK_dev_pub` / `SPK_dev_pub` 签名 | — | 链上 |

要点与理由：
- **`IK_dev` 设备专属、不派生自助记词**：使「持助记词者」也无法离线重算设备 DH 私钥（与 `devicePeerKey.ts` 注释
  「must be device-specific so a mnemonic holder cannot read handoff bundles」一致的安全边界）。设备丢失 → 换设备身份
  （撤销旧 `IK_dev`，§4.4 epoch）。
- **`SPK_dev` 可重算**：仅为换机后能恢复同一中期预密钥、减少链上重发；其泄露不破坏历史（DR 的 EK 临时钥兜底）。
- **`OPK` 必须随机且用后即删**：这是 X3DH 前向保密的硬要求，**任何可重算方案都禁止**。
- **HKDF 复用既有实现**：`vault_master` 与 `hkdfSha256` 已在 `nexchat/src/wallet/vaultMaster.ts` 落地并有冻结向量；
  新增 salt 串需同样写**冻结测试向量**（改动即破坏既有密文，按重大变更处理）。

> **v1 实装取舍（SPK）**：vodozemac 的 fallback key **随机生成**且不支持导入外部钥字节，故 v1 直接以 **vodozemac
> fallback key 作为 `SPK_dev`**（经 pickle 持久化跨刷新/恢复），暂不走 `nexchat/x3dh/spk/v1` 的 HKDF 可重算路径。
> §4.2 已注明 SPK 可重算仅为优化（非安全要求），故此为安全偏离；HKDF-SPK 留作后续优化（需 fork vodozemac 或自管 SPK）。
> 身份桥实装：`nexchat/src/crypto-dr/identityBridge.ts`（账户钥背书 `endorseKey`/`verifyEndorsement` + `publishPrekeyBundle`）
> 与 `opkMerkle.ts`（§19 Merkle 根/证明，冻结）。背书裸签复用 `signer.ts::signRawWithAccountKey`（与 E2EI 设备 leaf 同模式）。

### 4.3 链上 `pallet-msg-identity`（新增，轻量，按设备键控）
仅存身份预密钥锚点；**不存私聊内容、不存棘轮态、不参与加密运算**。

| 存储 | 内容 | 说明 |
|------|------|------|
| `DeviceIK: (AccountId, DeviceId) → (ik_x25519_pub[32], sig[64])` | 设备身份 DH 公钥 + 账户钥背书签名 | 对端据此做 X3DH 与 MITM 校验；`sig` 由账户 sr25519 钥签 `ctx‖ik_pub` |
| `DeviceSPK: (AccountId, DeviceId) → (spk_pub[32], sig[64], valid_until)` | 中期 SPK + 背书 + 过期 | 客户端定期轮换；链上 LWW + 时钟偏移上界（参考 EISA 防自锁） |
| `DeviceOPKRoot: (AccountId, DeviceId) → (merkle_root[32], count, epoch)` | OPK 集合 Merkle 根 | 链上只存根；OPK 叶子经 relay 分发，用一条标记一条（§19） |
| `PrekeyEpoch: (AccountId, DeviceId) → u64` | 预密钥轮换/撤销纪元 | 撤销/换设备递增，使旧 bundle 失效；对端/relay 据此拒绝陈旧 |
| `ChatStackCaps: AccountId → (flags: u8, version: u16)` | 1:1 栈能力位（DR / MLS-wire）+ 版本 | 模式协商（§20）：发起方据此选 DR 或回退 |

设计取舍：
- **OPK 只上根**（不逐条上链）：省存储、可验证「这条 OPK 属于该账户设备的已公布集合」；耗尽由客户端补货并更新根。
- **撤销**：本 pallet `PrekeyEpoch`（设备级）为主；账户级合规撤销可叠加 `pallet-chat-permission::CapabilityEpoch`。
- **押金 + 限频**：写入型 extrinsic 走 `ReservableCurrency` 押金 + `pallet-chat-common::rate_limit`（沿用 group/inbox 反垃圾模式）。
- **DeviceId 编码**：`DeviceId = blake2_128(ik_x25519_pub)`（16B），自证、与设备 DH 钥一一对应，避免额外注册。

### 4.4 不变量
- 链从不见 X3DH 共享秘密、根密钥、消息明文。
- `IK_dev` 一旦背书，轮换 `SPK`/`OPK` 不改 `IK_dev`；换 `IK_dev` = 强撤销（递增 `PrekeyEpoch` + 重新背书）。
- 所有 `IK_dev` / `SPK_dev` 公钥**必须**带有效账户钥背书签名，否则对端拒绝（防 relay 伪造预密钥，relay-trustless）。

---

## 5. 传输层（共用底座）/ Transport (shared)

复用现有 relay（`nexchat/relay-rs`），靠**路由键前缀分流**，不新建网络：

| 流量 | 路由键 | 投递语义 |
|------|--------|----------|
| 1:1 私聊（DR） | `d:{canonical_peer}`（UI）/ `d:{sorted_a}:{sorted_b}`（规范键） | 点对点/中继转发；**无 `commit_epoch`、无 CAS**（DR 无 epoch） |
| 群聊（MLS） | `g:{group_id}` | 不变（链上 `expected_epoch` 全序） |

要点：
- **DR 帧不带 `commit_epoch`，不走 commit-slot**：relay 原样扇出，并发由 DR 的消息序号天然处理（乱序/丢包由 DR skipped-message-keys 容错）。这比当前 MLS 1:1 的 relay 逻辑**更简单**。帧 schema 见 §21。
- **私聊密文投递鉴权**：复用 `pallet-chat-inbox` 的盲化一次性投递令牌（`inbox_id = H(IPK)`，relay 离线验签 + 撤销标签），与账户不可关联。
- **预密钥检索**：对端 `IK_dev`/`SPK_dev` 由链上读取；OPK 叶子经 relay 按需取一条（§19，relay 单发；libp2p-DHT 留作 Phase 2）。
- `s:<account>` 自通道沿用（多设备 presence / 离线补齐，见 §8 / §18.3）。
- **libp2p（可选，Phase 2）**：`/msg/private/1.0`、`/msg/group/1.0` 作为「去 relay 化」选项；v1 不强制。

### 5.1 与 commit-slot CAS 的关系
现有 relay 仅当 `d:` 帧携带明文 `commit_epoch` 时启用 commit-slot CAS（MLS Wire 用）。DR 帧**不带**该字段，
故 CAS 对 DR 流量空操作——严格加性、逐字节兼容既有 `d:` 透传，无需改既有 MLS 路径。

---

## 6. X3DH 握手（去中心化）/ X3DH handshake (decentralized)

参与方 A（发起）、B（接收）。`owner`（确定性较小地址，沿用 `directHandshakeOwner`）可用于无序双发时的去重。

```text
建立会话（A 主动给 B 发首条消息）：
  1. A 从链读 B 的 IK_B、SPK_B(+sig)；校验 sig（IK_B 验签 SPK_B）。
  2. A 经 relay/DHT 取 B 的一条 OPK_B（按 OpkRoot 标记消费；可缺省，缺 OPK 时降级为无 OPK 的 X3DH）。
  3. A 生成临时钥 EK_A，按 X3DH 计算：
       DH1 = DH(IK_A, SPK_B)
       DH2 = DH(EK_A, IK_B)
       DH3 = DH(EK_A, SPK_B)
       DH4 = DH(EK_A, OPK_B)            # 有 OPK 时
       SK  = KDF(DH1 ‖ DH2 ‖ DH3 ‖ DH4)
  4. A 用 SK 初始化 Double Ratchet（§7），首条消息含「initial message header」：
       { IK_A, EK_A, 消费的 OPK 标识, 协议版本, prekey_epoch_B }
  5. B 收到后用自有私钥重算同一 SK，完成 X3DH，进入同一棘轮根状态；标记该 OPK 已用。
```

要点：
- **去中心化**：无密钥服务器；预密钥真值在链（`IK_dev`/`SPK_dev`/根）+ relay（OPK 叶子，§19）。
- **背书校验**：取到的 `IK_B`/`SPK_B` **必须**带 B 账户钥的有效背书签名（§4.4），否则拒绝（relay-trustless）。
- **OPK 耗尽**：降级为「IK+SPK(fallback)」X3DH（前向保密略弱）→ 客户端按 §19 阈值补货并更新根。
- **MITM 防护**：可暴露 Safety Number（`IK_dev` 指纹比对），链上背书的 `IK_dev` 即可信锚。
- **会话标识**：`d:{sorted_a}:{sorted_b}`（与现有 relay 路由一致，便于复用投递面）。
- **库映射**：本流程经 vodozemac `Session::new_outbound` 实现，OPK/SPK 映射见 §17.1；字节头见 §18.1。

---

## 7. Double Ratchet 会话 / Double Ratchet session

每对 1:1 会话**独立一套棘轮状态**（A↔B 与 A↔C 完全无关）。

```text
两条棘轮：
  - DH ratchet：收到对端新 ratchet 公钥时换根密钥 → break-in recovery（PCS）
  - 对称棘轮（send/recv chain）：每条消息派生独立 message key → PFS

发消息：
  message_key = KDF_CK(send_chain_key); advance(send_chain_key)
  AEAD_encrypt(message_key, plaintext, header)     # header: ratchet 公钥、N、PN

收消息：
  按 header 推进/换链；缺失消息以 skipped-message-keys 暂存容错（上限 MAX_SKIP）
  AEAD_decrypt(message_key, ciphertext)
```

要点：
- **每条消息独立密钥**（PFS）；**DH ratchet 周期推进**实现 PCS（泄露后自愈、自动止损）。
- **乱序/丢包**：保留有界 skipped-message-keys；超界丢弃并提示（沿用 Signal 实践）。
- **持久化**：棘轮态（vodozemac `Session` pickle，密钥见 §17.2 `pickle/v1`）写入独立 `dr-session-store`（§9），随消息推进原子更新。
- **实现选型（决策）**：**vodozemac**（Olm `Session`）编入 wasm，**不自研原语**；仅做接线、序列化（§18）与存储。

---

## 8. 多设备 / Multi-device（最大工程块）

DR 多设备没有 MLS Wire 的「每设备 leaf + Add/Remove」标准路径，需显式设计。**v1 决策：方案 A（每设备独立 DR 会话）**，
理由是最简单且密码学上最安全（每条会话单一持有者、零共享发送链），UI 聚合在应用层处理；扇出/Sesame 留作 Phase 2。

| 方案 | 做法 | 取舍 | v1 |
|------|------|------|----|
| **A. 每设备独立 DR 会话** | 同账户每台设备各与对端每台设备建一条 DR（N×M 条，§18.3 路由） | 简单正确、零 nonce 风险；会话数膨胀、UI 需聚合 | ✅ 选用 |
| B. 账户内扇出 | 每设备独立 DR 与对端；本账户多设备经 `s:<account>` 同步会话快照（端内加密） | 复用自通道；需账户内安全同步信道 | Phase 2 |
| C. Sesame/Signal 设备管理 | 实现 Signal Sesame 设备会话管理 | 最完整；实现量最大 | Phase 2 |

关键红线：
- **绝不**让两台设备共享同一条 send chain 并各自发消息 → 相同 `(key, nonce)` → AEAD nonce 重用 → 机密性崩塌
  （与 `CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md` §3.1 同一密码学约束）。
- 每条 send chain **单一持有者**；多设备要么各自独立 chain（A/B），要么严格单写。
- 设备增删 = 身份层 + 会话层动作；新设备无法解历史实时消息，历史正文走 `K_archive`。

**兄弟设备 echo（方案 A 内实现，非方案 B）**：发送设备 a1 给对端 B 发消息时，除对 B 的每台设备各加密一份外，**也对本账户其他设备 a2/a3 各加密一份**（复用 a1↔a2 等 sibling DR 会话——兄弟只是「本账户的另一台设备」，仍每会话单一持有者、零共享链，红线不破）。这些 echo 帧在**原会话** `d:{对端}` 上路由并带 `echoSelf`（网络 relay 留存发送方邮箱供离线兄弟补齐），使兄弟设备把已发消息渲染进正确会话。这是方案 A 的自然扩展（N×M 含本账户内对），**不是**方案 B 的「同步会话快照」（仍 Phase 2）。代价：echo 帧也会被对端设备收到并因 `recv_dev` 不匹配而丢弃（有界浪费，随兄弟数线性）。

---

## 9. 本地存储隔离 / Local storage isolation

物理隔离，杜绝状态污染：

| 库 | 内容 | persistKey / 表 |
|----|------|-----------------|
| DR 会话库 | 每对设备会话的 vodozemac `Session` pickle（含根/链密钥、ratchet 私钥、N/PN、skipped-keys） | `dr:{account}`（独立命名空间，按 `{peer_account}#{peer_device}` 分条） |
| MLS 会话树库 | 群 OpenMLS 状态（不变） | `mls:{account}` / `g:*` |
| 身份库 | vodozemac `Account` pickle（`IK_dev`/`OPK` 私钥）、`SPK_dev` 私钥、MLS credential | `id:{account}#{device}`（高敏，单独加密） |
| 归档库 | `K_archive` 历史正文索引 | EISA 既有 |

规则：
- DR 与 MLS **不共用任何内存变量/单例引擎**（各自独立 engine 实例；编译期 import 边界，§3）。
- `IK_dev` 设备专属（不从 `vault_master` 派生，§4.2）；`OPK` 私钥**用后即删**（前向保密，§19）。
- 备份/恢复：会话态与 `IK_dev`/`OPK` **不进**助记词 vault（PFS/PCS 要求）；`SPK_dev` 可凭 vault 重算（§4.2）；历史可读凭 `K_archive` 自愈。

---

## 10. 与群聊解耦不变量 / Decoupling invariants vs group

1. **人数阈值**：恰好 2 人 → 强制 DR（**不建任何链上群**，与 `TwoMemberGroupForbidden` 一致）；≥3 人 → OpenMLS。
2. **状态隔离**：DR 棘轮态与 MLS 树态分库分引擎（§9），互不读写。
3. **变更逻辑分离**：DR 只有双方密钥滚动；MLS 是全局树形迭代 + 链上 epoch。两套更新路径无共享代码分支。
4. **链上分离**：DR 仅依赖 `pallet-msg-identity`（身份）；MLS 依赖 `pallet-chat-group`（DS/AS）。1:1 永不写群成员行（隐私）。

---

## 11. 2↔3 人切换编排 / Orchestrator (2↔3 transition)

DR 与 MLS **不可继承密钥**，切换处必须显式状态机；用户在 UI 无感，密钥层为新会话。

```text
invariant:
  participants == 2  → 仅 crypto-dr 活跃；无链上 create_group
  participants >= 3  → 仅 crypto-mls 活跃；该对的 DR 会话归档并停用
  历史展示           → 永远走 archive(K_archive)，不依赖当前 DR/MLS 密钥

2 → 3（A 邀 C 进 A-B 私聊）：
  1. orchestrator 冻结 A-B 的 DR 会话（停止新发，标记 archived）
  2. crypto-mls.createGroup({A,B,C})：链上 create_group + 三方 KeyPackage/Welcome/Commit
  3. 待全员入群成功 → 旧 DR 会话置 retired（密钥保留只读窗口后销毁）
  4. UI：同一会话视图无缝切到群；历史消息来自 archive

3 → 2（群仅剩 A、B）：
  1. crypto-mls.dissolve(group)：链上解散（成员降至合法值；禁止停在 2 人链上群）
  2. orchestrator 触发 A-B 重新 X3DH 握手（§6），建新 DR 会话（不继承群密钥）
  3. UI：提示「已转为私聊」（或全静默）；历史来自 archive

失败回滚：
  - 任一步失败 → 维持切换前状态（DR 仍活跃 / 群仍存），上报 UI 重试；
  - 绝不出现「DR 与 MLS 同时活跃于同一对」的中间态（用 orchestrator 单写锁 + 版本号收口）。
```

---

## 12. 链上改动与迁移 / On-chain changes & migration

- **新增** `pallet-msg-identity`（§4.2）：runtime 注册、runtime API（读 IK/SPK/OpkRoot）、node RPC 封装（`chat_*` 风格）。
- **不改** `pallet-chat-group`（群 MLS）；`permission` / `inbox` / `sync` 复用。
- **1:1 MLS 退役（迁移期）**：
  - 以 flag（如 `oneToOneCrypto = "dr" | "mls-wire"`）+ 会话版本号收口；同一对会话**二选一**。
  - 既有 MLS 1:1 会话：历史正文走 `K_archive`；新会话以 DR 建立；不做棘轮↔MLS 密钥迁移。
  - relay：`d:` 帧在 DR 模式下**不携带** `commit_epoch`（commit-slot CAS 对 DR 流量空操作，向后兼容）。

---

## 13. 安全属性与威胁模型 / Security properties & threat model

| 属性 | DR 1:1 | 说明 |
|------|--------|------|
| 前向保密 PFS | ✅ | 每条消息独立 message key，用后即删 |
| 泄露后自愈 PCS | ✅ | DH ratchet 周期换根，泄露后续自动止损 |
| 异步握手 | ✅ | X3DH，无需双方同时在线 |
| MITM 防护 | ✅（需用户比对或信任链上 `IK_dev`） | 链上账户钥背书的 `IK_dev` 作可信锚 + 可选 Safety Number |
| 元数据 | ⚠️ | relay 见化名路由键 `d:*`；账户不可关联依赖 inbox 盲令牌 |
| 多设备 nonce 安全 | ⚠️ 设计约束 | 严禁共享 send chain 并发发（§8 红线） |
| 预密钥伪造 | ✅ | `IK_dev`/`SPK_dev` 必带账户 sr25519 背书签名，relay 无法伪造（§4.4） |
| 内容上链 | ❌ 不发生 | 链只存身份预密钥锚点 |

链上面：`pallet-msg-identity` 写入须押金 + 限频；写入用**签名 origin**（账户即背书者），OPK 根更新天然由账户授权（防他人冒名补货）。

---

## 14. 测试计划 / Test plan

参考 `scripts/docs/NEXUS_TEST_PLAN.md` 与现有 `nexchat` 测试模式（WASM 实跑 + 纯逻辑单测）。

- **密码学单测**：X3DH（含 OPK 缺省降级）、DR 收发/乱序/丢包/skipped-keys 上限、PFS/PCS 断言。
- **解耦断言（关键）**：CI 强制 `crypto-dr` 与 `crypto-mls` 无互相 import；独立 persistKey；同一对不并存两态。
- **多设备**：选定方案的 nonce 安全回归（禁止共享 chain 并发发）。
- **切换 E2E**：2→3（DR 冻结→建群→收敛互解密）、3→2（解散→重握手→新 DR 互通）、失败回滚不产生双活态。
- **链上 pallet**：`pallet-msg-identity` 的 `DeviceIK` 背书校验、`DeviceSPK` 轮换 LWW、`DeviceOPKRoot` 消费/补货、`PrekeyEpoch` 撤销、`ChatStackCaps` 协商位；押金/限频；benchmarks。
- **relay**：`d:` DR 帧（`DmEnvelope`）无 `commit_epoch` 时的透传；`opk_publish`/`opk_fetch` 单发缓存；inbox 盲令牌投递鉴权。
- **历史自愈**：换设备后凭 `K_archive` 恢复历史正文，与会话密钥正交。

最低验证（CLAUDE.md）：至少跑直接受影响 crate（`pallet-msg-identity`）与 `nexchat` 的 `crypto-dr` 测试；
触达 relay/切换时补 E2E。

---

## 15. 开发分期 / Milestones

0. **M0 冻结向量** ✅：§17.2 域分隔串 + DeviceId 派生写成冻结测试向量（`pallet-msg-identity` tests + `dmEnvelope.test.ts` 跨语言）；§18 序列化 round-trip + golden。
1. **M1 身份底座** ✅：`pallet-msg-identity`（`DeviceIdentities`/`DeviceSignedPreKeys`/`DeviceOpkRoots`/`prekey_epoch`/`ChatStackCaps` + 签名 origin 授权 + 背书不透明存储 + 押金/设备上限）+ runtime API + 测试 + runtime 接线（index 80）。
2. **M2 DR 链路**：vodozemac wasm 接线 ✅（`dr-wasm`/`VodozemacEngine`，X3DH/Olm + §18 序列化 + e2e 测试）；客户端身份桥 ✅（`identityBridge.ts`：账户钥背书 + `opkMerkle` §19 + `publishPrekeyBundle` 调 `register_device`/`set_signed_prekey`/`set_opk_root`/`set_stack_caps`）；对端预密钥取回 ✅（`prekeyFetch.ts`：`ChainClient` 读 `msgIdentity` devices/SPK/OPKRoot/caps + `assemblePeerBundle` relay-trustless 背书校验 + §20 `chooseStack`/`negotiateStack`）；relay `d:` 投递 ✅（`drTransport.ts`：复用 `RelayFrame`、`convId=d:{peer}`、`base64(DmEnvelope)`、无 commit-slot CAS、`recvDev` 多设备路由 §18.3 + e2e 环回测试）；DR session-store 持久化 ✅（`sessionStore.ts`：`EncryptedDrSessionStore`，vault 派生钥 `nexchat/x3dh/pickle/v1` 加密账户/会话 pickle + `restoreDrTransport` 跨重启恢复棘轮态）；OPK 控制面单发 ✅（`opkExchange.ts`：`OpkResponder` 按 best-effort 已用集合单发叶子 + 证明、`requestOpk` 取回并对链上根校验、`fetchPeerBundleWithOpk` 升级到 OPK 否则 SPK 回退；§21 `opk_publish`/`opk_fetch` 帧已加入 `ControlMsg`）。**待续（仓外）**：`relay-rs` 侧 OPK 内存缓存（持有者离线时代发，§21「实现状态」），当前由持有者在线按需服务、取不到则 SPK 回退。
3. **M3 解耦 + 协商** ✅：编译期 import 边界 ✅（`drTransport` 去除 `@/mls/directConv` 依赖、内联 `d:` conv-id；`decoupling.test.ts` 扫描断言 `@/crypto-dr/*` ⟂ `@/mls/*` 双向零 import）；独立存储 ✅（DR 专用 IndexedDB `nexchat-dr-*` + 专属 HKDF 上下文 `nexchat/x3dh/pickle/v1`，与 MLS 密钥空间隔离）；二选一收口 ✅（`convStack.ts`：`ConvStackRegistry` + `resolveConvStack` 首次按 §20 协商即钉定、此后 sticky、`none` 不钉定可重试、迁移用 `renegotiate`）；`ChatStackCaps` 模式协商 ✅（§20 `negotiateStack` 接入 `resolveConvStack`）。**待应用接线**：把 `resolveConvStack` 接进 orchestrator 的"开会话"决策（M5）。
4. **M4 多设备（方案 A）** ✅：每设备独立 DR 会话（引擎按对端 DeviceId 键控会话，已有）+ §18.3 扇出路由 ✅（`multiDevice.ts`：`MultiDeviceRouter.sendToAccount` 发现对端全部设备、按设备懒建会话、各加密各发一份；`DeviceDirectory`/`ChainDeviceDirectory` 设备发现、`PeerBundleProvider`/`RelayBundleProvider` 按设备装包；入站 `recv_dev` 路由已由 `DrTransport` 强制）+ nonce 安全回归 ✅（`multiDevice.test.ts`：每设备独立密文、跨设备会话不可互解、重发各自推进链无 nonce 复用、排除本设备防自发环、无 bundle 设备跳过）。兄弟设备 echo ✅（`DrTransport.sendTo` 加 `{convId?,echoSelf?}`：兄弟回显寻址兄弟**设备**但按**原会话** `d:{对端}` 路由 + `echoSelf` 留存发送方邮箱；`MultiDeviceRouter.sendToAccount` 透传 opts；`DrIncoming.convId` 携带帧路由 conv-id，使自发回显落入正确会话；`multiDevice.test.ts` 加兄弟 echo 用例 = 6 例）。设备目录刷新订阅 ✅（`ChainDeviceDirectory` 短 TTL 缓存 + `refresh`/`invalidate` + `subscribeRefresh(relay)`：对端 `opk_publish` 使该账户缓存失效，下次扇出重读纳入新设备；`onControl` 扇出故与他者共存；`deviceDirectory.test.ts` 2 例：TTL 缓存/失效重读、订阅幂等）。
5. **M5 orchestrator** ✅：2↔3 切换状态机 + 失败回滚 + archive 接线 + E2E。新增 `src/orchestrator/`（唯一允许同时依赖 `@/crypto-dr/*` 与 `@/mls/*` 的模块，`decoupling.test.ts` 只扫两引擎目录故仍绿）：六边形端口 ✅（`ports.ts`：`DrSessionPort`(open/freeze/resume/retire/isActive)、`MlsGroupPort`(createGroup/dissolve/isActive)、`ArchivePort`(archive)——编排器只调公开入口，绝不读密钥/会话库 §2.3）；状态机 ✅（`chatOrchestrator.ts`：每会话单写锁 + 单调版本号收口；2→3 `archive→dr.freeze→mls.createGroup→(成功 dr.retire | 失败 dr.resume)`；3→2 `archive→mls.dissolve(枢轴)→dr.open`，dissolve 失败完整回滚、open 失败提交为 direct 并 `ensureDirect` 重试；硬不变量「绝不 DR 与群同对双活」）；三真实适配器 ✅：DR `drSessionAdapter.ts`（包 `MultiDeviceRouter`，持活跃/冻结账本并门控出站发送，冻结/退役期 `send` 抛错防泄漏到已冻结栈）、MLS `mlsGroupAdapter.ts`（`MlsGroupPort` 包既有 `createGroupWithMembers`/`disbandGroup`/`engine.hasGroup`——只调公开流程不读 epoch/密钥；非群主解散抛错即由编排器回滚保群）、Archive `archiveAdapter.ts`（`MsgArchivePort` 包 `MsgArchiveSync.push`，切换边界刷 `K_archive` 历史快照，账户级幂等）；E2E ✅（`chatOrchestrator.test.ts` 7 例：2→3 顺/回滚、3→2 顺/dissolve 回滚/open 失败重试、并发锁、模式前置校验 + 每步断言不双活；`drSessionAdapter.test.ts` 3 例门控；`mlsGroupAdapter.test.ts` 4 例 deps 映射/默认群名/isActive；`archiveAdapter.test.ts` 2 例 push 透传/失败传播；共 16 例全绿）。App 启动装配 ✅（`assembleChatStack.ts`：组合根，把既有单例 relay/chain/`openMlsEngine`/`localStore` 装配成 `restoreDrTransport`（跨重启恢复棘轮）+ `ChainDeviceDirectory`/`RelayBundleProvider`/`MultiDeviceRouter` + `DrSessionAdapter`/`MlsGroupAdapter`/`MsgArchivePort` + `ChatOrchestrator` + `ConvStackRegistry`；`appStore.unlock()` 在 relay/keyVault/localStore 就绪后按 `config.drEnabled`（默认关，需 relay）best-effort 调用——DR 初始化失败绝不中断 unlock，1:1 回退 MLS-Wire；`getChatStack()` 暴露给后续会话决策/切换钩子；`assembleChatStack.test.ts` 2 例：装配产物 + 跨重启同设备身份恢复）。收发分流 ✅（`appStore.ts`）：同步快表 `drPinnedPeers` + `isDrPeer`/`pinConvStack`（开会话经 `resolveConvStack` §20 协商即钉定 DR，热路径同步分流，无异步链读）；`openConversation`/`startDirectChat` 对 DR 钉定对端跳过 MLS 握手（不发 MLS 控制帧）；出站 `drSend`（DR 钉定 → `router.sendToAccount` 扇出 `encodeEnvelope(env)`，否则 MLS-Wire）已接入 text/file/recall/media_ack/forward(`persistAndRelay`) 全部 1:1 出站点；入站单一 relay 订阅 `handleInbound` 作分发器（不调 `transport.attach()`），先 `transport.ingestFrame(frame)`（`DrTransport.handleFrame` 改返 boolean 判别：为我解密/兄弟设备帧 → true 消费、非 DR → false 回退 MLS），DR 帧经 `transport.onMessage`→`depositDrMessage`→共享 `depositInboundEnvelope`（从 `handleInbound` 抽出，MLS 与 DR 共用：media_ack/recall/去重/未读/刷新）入库；就绪门 `isMlsReady` 对 DR 对端直返 true（router 懒建会话，无阻塞握手）；全程受 `chatStack` 非空门控（`config.drEnabled` 默认关 → 行为逐字节不变）。`drTransport.test.ts` 加 `ingestFrame` 判别用例（共 5 例）。M2 身份桥启动发布 ✅（`appStore.ts`）：`publishDrPrekeysInBackground` 在 `runPostUnlockBackgroundWork` 内非阻塞调用 `publishPrekeyBundle(engine, { store })`（注册 IK + SPK + OPK Merkle 根 + `set_stack_caps` 公告 DR 能力 §20），与 KeyPackage 发布同位；一次性守卫 `drPrekeysPublished`（每解锁一次）+ 解锁时连同 `drPinnedPeers` 重置；mock 或签名者不可裸签（`endorseKey` 需要）时跳过，失败仅使 1:1 维持 MLS-Wire 至能力上链。至此两端开启 `drEnabled` + 可裸签即可经 §20 协商点亮 DR（SPK 回退 X3DH）。`OpkResponder` 控制面接线 ✅（`assembleChatStack` 内对本设备 `attach()`；`onControl` 在 BC/MUX/WS 三传输均为扇出，故与 `DirectMlsRegistry` 共存、**无需控制面多路复用**）。兄弟设备 echo 接线 ✅（`drSend` 在对端扇出后追发 `router.sendToAccount(self, plaintext, {convId, echoSelf:true})`，非阻塞、不影响对端投递；`depositDrMessage` 自发分支按 `m.convId`（= `d:{对端}`）以自发消息落入会话）。DR 就绪态 UI 徽标 ✅（`appStore` 加响应式 `drPeers: Record<addr,bool>` 映射 + `markDrPeer`/`unmarkDrPeer`，由 `pinConvStack`/入站自动钉定/解锁重置维护；`isMlsReady`/`ChatWindow.directReady` 对 DR 对端直返就绪；`ContactsPanel`/`ContactDetail`/`NewDirectChat`/`ProfilePanel` 显示「私聊 E2EE 就绪 (DR)」徽标）。
6. **M6 优化/可选**：OPK 严格一次性（spent-set）、libp2p `/msg/private` `/msg/group` 分流、扇出/Sesame 多设备、性能基准。

---

## 16. 已冻结决策（原开放问题）/ Frozen decisions

| 原开放问题 | 决策 | 依据 / 章节 |
|------------|------|-------------|
| IK 复用账户钥还是独立钥？ | **独立 X25519 设备钥 + 账户 sr25519 背书**（账户钥不能做 X25519 DH） | §4.1 / §4.2；仿 `devicePeerKey.ts` |
| 密码学库 | **vodozemac（Olm Account/Session）** 编入 wasm，不自研原语 | §17；映射见 §17.1 |
| OPK 分发 | **纯 relay v1**（在线分发 + best-effort 一次性）；libp2p-DHT 留作 Phase 2 | §19 / §5.1 |
| OPK 防双花 | relay best-effort 单发 + 接收方用后即删 + Merkle 根证成员；OPK 重用仅弱化该首条 FS（X3DH 已知性质），不破会话 | §19 |
| 多设备方案 | **方案 A：每设备独立 DR 会话**（account×device 笛卡尔对），v1 不做明文扇出同步；Sesame(C) 留作 Phase 2 | §8 / §18.3 |
| 模式协商 | 链上 `ChatStackCaps` 公布能力位 + 握手内 1B 版本；发起方按对端能力选 DR / 回退 MLS-wire | §20 |
| 撤销锚统一 | 设备级 `PrekeyEpoch` 为主，账户级合规撤销叠加 `CapabilityEpoch` | §4.3 |
| 迁移并存 | 允许「DR 与 MLS-wire 长期并存、按对端能力协商」；不设硬切窗口；同一对会话二选一 | §12 / §20 |
| AEAD 套件 | **XChaCha20-Poly1305**（24B nonce，可随机 nonce；与 `crypto-common` 枚举一致） | §18 |
| KDF | **HKDF-SHA256**（与 `vaultMaster`/`devicePeerKey` 一致），域分隔串见 §17.2 | §17 / §18 |

---

## 17. 密码学库与原语 / Crypto library & primitives（决策）

**选定 `vodozemac`**（Matrix 的 Rust 实现，已审计，可编 wasm，含 Olm = X3DH 等价 + Double Ratchet）。理由：避免自研
原语、与现有 `mls-wasm` 同类的 wasm 依赖模式、Curve25519 与 X3DH 语义对齐。

> **实装（M2 已完成）**：`nexchat/dr-wasm/`（crate `nexchat-dr`，vodozemac 0.10）封装为 wasm，导出 `DrClient`；
> TS 引擎 `nexchat/src/crypto-dr/vodozemacEngine.ts`（`VodozemacEngine implements DrEngine`）。构建：`npm run dr:build`
> （产物落 `src/dr-pkg/`，随源提交，同 `mls-pkg`）。端到端测试见 `vodozemacEngine.test.ts`。与 `mls-wasm` 物理隔离、无交叉 import。

### 17.1 Olm ↔ 本设计映射
| 本设计 | Olm/vodozemac | 说明 |
|--------|---------------|------|
| `IK_dev` (X25519) | `Account` identity key (Curve25519) | 设备身份 DH 钥 |
| `SPK_dev` | —（Olm 无独立 SPK） | **本设计在链上加 SPK 层**：用作"无 OPK 时"的回退 DH 目标；Olm 侧以 fallback key 表达 |
| `OPK_dev[i]` | `Account::one_time_keys()` | 一次性预密钥；`mark_keys_as_published` 后上根（`DrClient.oneTimeKeys`/`generateOneTimeKeys`） |
| X3DH 首条 | `Account::create_outbound_session` + `PreKeyMessage` | 发起方用对端 (IK, OPK 或 fallback) 建出站会话（`DrClient.createOutboundSession`） |
| X3DH 应答 | `Account::create_inbound_session` | 应答方由 `dm_init`（PreKeyMessage）建入站会话并取回首条明文（`DrClient.createInboundSession`） |
| 后续消息 | `Session::encrypt`/`decrypt`（Olm ratchet） | Olm 内置 Double Ratchet（`DrClient.encrypt`/`decrypt`） |

> 注：Olm 的 fallback key 语义 ≈ 本设计 `SPK`（OPK 耗尽时的回退）；若严格 Signal X3DH（含 SPK 签名进 KDF）为硬需求，
> 则改用 `libsignal` 绑定（备选），但实现量更大。v1 取 vodozemac。

### 17.2 域分隔串（冻结）
所有 HKDF/签名上下文加固定前缀，写入冻结测试向量：
- vault 派生 SPK：`"nexchat/x3dh/spk/v1"`
- 账户背书 IK：`"nexchat/x3dh/ik-endorse/v1"`
- 账户背书 SPK：`"nexchat/x3dh/spk-endorse/v1"`
- DR 消息 AEAD AAD：见 §18.2
- Olm pickle 加密（落库）：`"nexchat/x3dh/pickle/v1"`

---

## 18. DR 消息序列化 / Double Ratchet wire format（冻结）

> 对标 `CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md` 的字节级精度。所有整数 **little-endian**；版本前置便于演进。

### 18.1 帧体（放入 `RelayFrame.ciphertextB64` 解 base64 后的字节）

参考实现：`nexchat/src/crypto-dr/dmEnvelope.ts`（`encodeDmEnvelope`/`decodeDmEnvelope`/`peekDmHeader`）。
固定小端布局（SCALE 等价参考框架，定长头 46 字节 + `u32 LE` body 长度前缀）：
```
DmEnvelope (little-endian, header = 46 bytes):
  off  0  ver:          u8        # = 1
  off  1  kind:         u8        # 0 = dm_init(首条, 携 X3DH/PreKeyMessage) | 1 = dm_msg(后续)
  off  2  sender_dev:   [u8;16]   # 发送方 DeviceId = blake2_128(IK_dev_pub)
  off 18  recv_dev:     [u8;16]   # 接收方 DeviceId（多设备路由用）
  off 34  prekey_epoch: u64 LE    # 发送方所见对端 PrekeyEpoch（陈旧即触发重取/拒绝）
  off 42  body_len:     u32 LE    # body 字节数
  off 46  body:         [u8;body_len]  # kind=0: Olm PreKeyMessage 字节; kind=1: Olm Message 字节
```
- 前 42 字节（`ver`/`kind`/`sender_dev`/`recv_dev`/`prekey_epoch`）为**明文路由头**（relay 路由用，无内容泄漏）。
- `body` 是 vodozemac 产出的不透明密文（Olm 自带 ratchet header：ratchet pub、N、PN）。
- **不带 `commit_epoch`**：DR 无 epoch，relay 不做 commit-slot CAS（§5.1）。

### 18.2 AEAD / AAD
- AEAD = **XChaCha20-Poly1305**（由 Olm 内部按消息派生 (key, nonce)）。
- 外层 AAD（若客户端在 Olm 之上再封一层用于绑定头）= `blake2_256(ver‖kind‖sender_dev‖recv_dev‖prekey_epoch‖convId)`，
  防头被篡改重路由。v1 可省（Olm 已 AEAD），列为可选硬化。

### 18.3 多设备路由（方案 A）
- 账户 A（设备 a1,a2）↔ 账户 B（设备 b1）：维护 a1↔b1、a2↔b1 两条独立 Olm 会话。
- 发消息：对对端**每个已知设备**各加密一份，relay 经 `routeTo`/`d:` 扇出；本账户兄弟设备经 `echoSelf` 收副本（明文不出端，副本是各自会话密文或本端 re-encrypt）。
- **nonce 红线**：每条 Olm 会话单一持有者，绝不跨设备共享发送链（§8）。

---

## 19. OPK 生命周期 / One-time prekey lifecycle（冻结）

```
发布：
  1. 客户端 Account 生成 N 条 OPK（建议 N=100），取公钥集合 {opk_pub[i]}
  2. merkle_root = merkle(sorted(opk_pub[i]))；submit set_opk_root(device, root, count=N, epoch++)
  3. OPK 公钥叶子 + 成员证明经 relay 控制面发布（ControlMsg 扩展 opk_publish），relay 缓存待取
取用（发起方 A 找 B 的 OPK）：
  1. A 向 relay 请求 B.device 的一条未发 OPK（relay best-effort 单发，标记该叶子已派发）
  2. relay 回 { opk_pub, merkle_proof }；A 用 DeviceOPKRoot 链上根验证 proof（relay-trustless）
  3. A 以 (IK_B, SPK_B 或 OPK_B) 建出站 Olm 会话，dm_init 头携带所用 OPK 标识
消费（接收方 B）：
  1. B 收 dm_init，用对应 OPK 私钥建入站会话，随即**删除该 OPK 私钥**（前向保密）
  2. B 监测剩余 count，低于阈值（建议 < 20）→ 触发补货（回到"发布"，epoch++）
防双花：
  - relay 单发为 best-effort；同一 OPK 被两发起方取走 → 仅这两条首条消息共享该 DH 分量（X3DH 已知弱化，
    会话仍因 EK 临时钥而独立安全）；不视为协议破坏。
  - 严格一次性（如需）= Phase 2：接收方侧"已用 OPK 集合"+ 拒绝重复（务实降级，参考 inbox spent-set）。
```

---

## 20. 模式协商 / Stack negotiation（冻结，防互通断裂）

```
ChatStackCaps.flags 位：bit0 = supports_dr, bit1 = supports_mls_wire
发起方 A 要给 B 发 1:1：
  1. 读 B 的 ChatStackCaps（链上，无则默认仅 mls_wire = 旧客户端）
  2. if B.supports_dr && A.supports_dr → 走 DR（本规范）
     elif B.supports_mls_wire && A.supports_mls_wire → 回退 pairwise MLS Wire（现状）
     else → UI 提示"对方客户端版本不兼容，无法私聊"
  3. 选定后写入会话索引的 stack 字段（二选一，不并存；§12）
握手内冗余：dm_init.ver 提供 1B 版本，防 caps 缓存陈旧导致错配（收方 ver 不识别即拒并回提示）。
```
- **互通矩阵**：DR↔DR=DR；MLS↔MLS=MLS；DR↔仅MLS=回退 MLS（双方都支持 MLS 时）或不可通。
- 迁移期客户端**应同时支持**两栈（双实现并存）；**新客户端默认 DR 优先**（`VITE_DR_ENABLED` 默认开，§20 `chooseStack` 双方支持则选 DR），老客户端不升级则维持 MLS-Wire 互通。

---

## 21. relay 帧 schema diff / Relay schema additions（冻结）

复用 `RelayFrame`（`convId`/`senderRef`/`ciphertextB64`/`delivery`/`routeTo`/`echoSelf`），1:1 DR **不新增帧结构**，
仅约定：

| 项 | 约定 |
|----|------|
| `convId` | `d:{canonical_peer}`（UI）/ 规范键 `d:{sorted_a}:{sorted_b}`（沿用 `directConv.ts`） |
| `ciphertextB64` | base64(`DmEnvelope`，§18.1)；relay 不解码 |
| `delivery` | 复用 RFC 9474 盲签投递准入（`pallet-chat-inbox`），与账户不可关联 |
| commit-slot CAS | **不触发**：DR 帧无 `commit_epoch`，relay 原样存转（向后兼容现有 `d:` 透传） |
| 留存 | 同普通应用消息 `RETAIN_MAX`；离线补齐经 `echoSelf` 账户邮箱 + backlog |

`ControlMsg` 新增（控制面，仅这两条；其余 MLS 专用控制消息在 DR 会话中不使用）：
```
| { t: "opk_publish"; device_id: string; root: string; leaves: Array<{ opk_pub: string; proof: string }> }
| { t: "opk_fetch"; convId: string; target_device: string }   // 回 opk_publish 的单条
```
- relay 对 `opk_*` 只做缓存/单发，不解析密码学；鉴权沿用 `register_account` WS 会话。
- **磁盘 parity 红线**：若 OPK 缓存需持久化，须评估对 `relay-rs` `core/tests/js_compat.rs` 的影响；
  建议 OPK 缓存为**内存态**（仿 commit-slot），不破磁盘格式。
- **实现状态**：`opk_publish`/`opk_fetch` 两条已加入客户端 `ControlMsg`（`relayClient.ts`，schema 冻结）；
  **客户端控制面单发已实装**（`opkExchange.ts`）：持有者 `OpkResponder` 收到 `opk_fetch` 即从已发布集合
  单发一条未用叶子 + Merkle 证明（best-effort 已用集合持久化于 `sessionStore`），发起方 `requestOpk`
  对链上根校验后用于 X3DH（`fetchPeerBundleWithOpk` 取不到则按 §6 回退 SPK）。
  **控制面已接线**：`OpkResponder` 在 `assembleChatStack` 内对本设备 `attach()`（响应方）；发起方侧经
  `RelayBundleProvider`/`fetchPeerBundleWithOpk` 走 OPK。**无需控制面多路复用**——`onControl` 在三种 relay
  传输（`BroadcastChannelRelay`/`MultiplexRelay`/`WebSocketRelay`）上均为**扇出**（`ctrlCbs` 列表，MUX/WS
  内容去重），故 `OpkResponder` 与 MLS 控制消费者（`DirectMlsRegistry`）天然共存，互不覆盖。
  **relay 侧 OPK 内存缓存已实装（OPK-over-relay，Phase 2 收口）**（`relay-rs`：`server/src/state.rs`
  `Inner.opk_cache` + `opk_cache_publish`/`opk_cache_dispense`，`server/src/protocol.rs` 控制面）：
  - 持有者解锁发布预密钥后经 `OpkResponder.upload()`（`opk_publish` 无 `toAddr`、走 `s:<self>`）把**未用
    叶子全集**上传 relay；relay 按 `(认证账户, device)` 键缓存（上传者为 WS 会话认证账户，外部账户无法污染
    他人槽位）。
  - 发起方 `opk_fetch` 时 relay **从缓存单发**一条叶子（`VecDeque` 先进先出 + `dispensed` 集合保证**单生命
    周期内严格单发**），直投**发起方账户**（非持有者）——故**持有者离线也能服务**首条 X3DH。
  - 缓存未命中（未上传/已耗尽）时 relay 把 fetch **转发在线持有者**，其 `OpkResponder.serve` 以
    `opk_publish{toAddr: 发起方}` 实时回复，经 relay `toAddr` 路由（仿 `token_req`/`token_sig`）回发起方；
    持有者离线则不存（追不上发起方超时）→ 发起方按 §6 回退 SPK。
  - **磁盘 parity 红线遵守**：缓存为**纯内存态**（仿 `commit_slots`，从不序列化），重启等持有者重新上传即可，
    `core/tests/js_compat.rs` 不受影响；跨重启的重复派发属 §19 记录的 best-effort。
  - 上限：每设备 `OPK_MAX_LEAVES_PER_DEVICE=256`、全局 `OPK_MAX_DEVICES=8192`（内存 DoS 护栏）。
  - 测试：`relay-rs` `state.rs`（单发 + 重传排除已派发 + 上限）、`protocol.rs`
    （上传→单发到发起方→耗尽转发持有者）；客户端 `opkExchange.test.ts`（`buildOpkLeaves` + `upload` + `serve` toAddr）。

---

> 注：本文 v2 已将 v1 的开放问题冻结为可实现决策（§16）并补齐字节级规范（§17–§21）。
> 仍属**替代栈**：是否采用取决于产品对「1:1 更轻量 / Signal 风格」与「重写 1:1 + 多设备」成本的权衡；
> 群聊 OpenMLS 路线在任一选择下均不变。开工前建议把 §17.2 域分隔串与 §4.2 派生写成**冻结测试向量**（对标 `vaultMaster.test.ts`）。
