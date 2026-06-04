# Chat 链下投递 · 盲化一次性投递令牌规范

> 状态：设计 / 待评审（**密码学协议，先评审后落地**）
> 适用范围：全链下人类消息的投递准入（联系人信箱）+ relay 反垃圾 + sealed-sender
> 关联：`CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md`（MLS payload 信封）、
> `CHAT_LARGE_FILE_SPEC.md`（大文件信封）、
> `pallets/chat/permission/`（`CapabilityEpoch` 撤销锚、已落地）、
> `pallets/chat/group/`（MLS DS/AS、身份/群状态定序）、
> `pallets/storage/service/`（relay/Pin 计费激励）
> 定稿决策（本文档前提）：
> 1. **per-contact 子钥/标签**——支持定向撤销单个联系人；
> 2. **一次性降级**——接受「relay 侧至多 k 份超花 + Bob 客户端按 `t` 去重」，不追求跨 relay 全局严格一次性。

---

## 0. 一句话结论 / TL;DR

CN: 在「人类消息全链下」模型里，**投递准入**不靠链上 gas，而靠接收方预先签发、可公开验证、
一次性使用、对 relay 隐藏发送方的**盲化投递令牌**。基线用 **RFC 9474 Blind RSA（RSABSSA）**：
接收方盲签令牌给联系人；relay 用接收方公钥离线验签 + per-inbox spent set 去重；
撤销靠 **per-contact 标签 + 整批 epoch**（`CapabilityEpoch` 已在链上）。严格一次性被务实降级为
「Bob 侧精确一次 + relay 侧至多 k 份」。

EN: With all human messages off-chain, delivery admission is gated not by on-chain gas but by a
receiver-issued, publicly verifiable, one-time, sender-hiding **blinded delivery token**. Baseline:
**RFC 9474 Blind RSA (RSABSSA)** — the receiver blind-signs tokens for a contact; a relay verifies
them offline with the receiver's public key plus a per-inbox spent set; revocation uses a
**per-contact tag + epoch** (`CapabilityEpoch` already on-chain). Strict one-time-ness is pragmatically
downgraded to "exactly-once at Bob's client + at most k copies across relays".

---

## 1. 为什么是这个原语 / Why this primitive

硬性需求（缺一不可）：

1. **可公开验证** —— relay 用一个公钥即可验，**不联系接收方**（接收方可离线）。
2. **不可伪造授权** —— 仅接收方授权过的发送方能投递。
3. **一次性** —— 每令牌一次，作为反垃圾的量化闸门。
4. **发送方匿名** —— relay 验令牌时学不到发送方身份（sealed-sender）。
5. **可撤销** —— 绑 `CapabilityEpoch`；并支持**定向**撤销单个联系人。
6. **离线异步签发** —— 接收方一次（批量）签发，发送方自行使用。
7. **每条消息零链上操作**。

需求 1 排除了 VOPRF / KVAC（RFC 9497）：其验证需 issuer 私钥，而我们的验证方是 relay ≠ 接收方
且接收方离线。**必须公开可验证 ⇒ 盲签名**。基线选 **Blind RSA（RFC 9474）**：成熟规范、无配对、
relay 仅需公钥即可验。BBS+ 匿名凭证列为升级路径（§11）。

---

## 2. 角色与符号 / Roles & notation

| 符号 | 含义 |
|---|---|
| Bob | 接收方（令牌**签发者**、信箱拥有者） |
| Alice | 发送方（Bob 的联系人，令牌**持有者/兑付者**） |
| Relay | 链下 store-and-forward 节点（验证者兼任），令牌**验证者** |
| `inbox_id` | Bob 的可轮换、不可关联联系人信箱 id（链上按 id 存，不暴露 AccountId） |
| `epoch` | `InboxRecord.epoch`，inbox 维度整批撤销计数器（`pallet-chat-inbox` 已落地；**≠** 账户级 `CapabilityEpoch`） |
| `IPK_e = (N, e)` | Bob 在 `epoch` 下的 Blind-RSA 签发**公钥**；**不上链**，由发送方携带、经 `inbox_id = H(IPK)` 自验证；私钥 `d` 在 Bob 设备 |
| `ct_c` | 联系人 c 的**标签**（32B 随机），Bob 分配并发给 Alice；定向撤销的判别符 |
| `t` | 令牌 nonce（32B 随机，Alice 生成，盲化后 Bob 看不到） |
| `s` | 令牌签名 `= H(t ‖ ct_c ‖ epoch)^d mod N` |
| `n` | Bob 一次签发给某联系人的令牌张数（反垃圾上限） |
| `k` | 信箱在 relay 间的复制因子（可用性） |

令牌 = `(epoch, ct_c, t, s)`。

---

## 3. 链上锚面 / On-chain anchors（v1 已落地 `pallet-chat-inbox`）

inbox 注册表已实现于 **`pallets/chat/inbox/`（`pallet-chat-inbox`, runtime index 78）**。
落地后字段与原设计有一处关键收敛——**IPK 不再上链**：

```text
inbox_id ──▶ InboxRecord {
    controller:    AccountId           // 注册并支付押金的控制账户（应为一次性钥）
    epoch:         u32                 // inbox 维度撤销纪元（≠ 账户级 CapabilityEpoch）
    revoked_tags:  BoundedVec<ct>      // 已撤销联系人标签（定向撤销；bump epoch 清空）
    deposit:       Balance             // 注册预留、注销退还
    created_at:    BlockNumber
}
```

落地决策（与原 §3 字面设计的差异，均为消解「链上注册表 ⊥ 不可关联」冲突）：

- **IPK 下链、不进注册表**：链不存 `(N,e)`、永不做 RSA。`inbox_id` 在**链下**绑定签发公钥
  （`inbox_id = H(IPK ‖ salt)`）；发送方兑付时随令牌携带 `IPK`，relay 校验 `inbox_id == H(IPK)`
  后再验签。链上注册表因此无需 `ipk_by_epoch`，匿名上限不依赖「注册表是否泄露账户」这一前提的
  公钥部分（见 §9）。
- **epoch 改为 inbox 维度**：撤销纪元存于 `InboxRecord.epoch`、以 `inbox_id` 为键。**不复用**
  `pallet-chat-permission::CapabilityEpoch[AccountId]`——后者按账户存，relay 读取会把 inbox 链回账户，
  与不可关联冲突。两个 epoch 正交：账户级留给合规/场景层，inbox 级服务投递令牌。
- **v1 反垃圾 = 签名 + 押金**：`register_inbox/bump_epoch/revoke_tag/deregister_inbox` 均为
  **签名** extrinsic，由 `controller` 支付预留押金（`InboxDeposit`，默认 0.5 NEX）。代价：链上出现
  `inbox_id → controller_account` 关联。**为不可关联，`controller` 必须是与拥有者主聊天账户无关的
  一次性钥**；relay 只读 inbox 维度 `epoch/revoked_tags`、从不读 `controller`。账户无关（unsigned +
  inbox 钥签名 + PoW）注册列为后续加固（见 §10.7）。
- `revoked_tags` 由 `MaxRevokedTags`（默认 256）上限约束；`bump_epoch` 整表清空。
- 每条消息**不**触链；链只在「注册 / 换 epoch / 定向撤销 / 注销」时被写。

读取面（relay 离线校验用）：runtime API `ChatInboxApi { inbox_epoch, is_tag_revoked, inbox_exists }`
与 JSON-RPC `chat_inboxEpoch / chat_isTagRevoked / chat_inboxExists`。

---

## 4. 密钥与标签模型 / Key & tag model

- **每 epoch 一把 Blind-RSA 签发密钥** `IPK_e`。换 epoch（`bump_capability_epoch`）= 发布新公钥，
  旧令牌因验签用当前 epoch 公钥而**整批作废**（核弹式撤销）。
- **每联系人一个标签** `ct_c`（不是每联系人一把 RSA 密钥——避免在链上堆多个模数）。`ct_c` 绑入签名消息
  `H(t ‖ ct_c ‖ epoch)`，并在兑付时**明文呈现**给 relay 以便过滤撤销表。
- **定向撤销** = Bob 把 `ct_c` 加入 `revoked_tags[inbox_id]`；relay 拒绝该标签的令牌。其余联系人不受影响、
  无需重签。

> 取舍（已拍板接受）：`ct_c` 对 relay 可见 ⇒ relay 能把**同一联系人**的多条消息聚类为同一伪名
> （仍不知道是谁）。这是「定向撤销」的必然代价：选择性撤销需要一个 relay 可见的判别符，而该判别符
> 必然让该联系人的流量可被伪名聚类。未撤销联系人的 `ct_c` 在 relay 眼中只是随机值，不可归因到身份。

---

## 5. 签发协议 / Issuance（Bob → Alice，批量、可异步）

前提：Bob 已通过既有安全信道（MLS 1:1 会话 / 带外）认 Alice 为联系人，并分配 `ct_c`。

```text
Alice:  for i in 1..n:
            选随机 t_i
            盲化  b_i = H(t_i ‖ ct_c ‖ epoch) · r_i^e   mod N        # r_i 随机盲化因子
        → 发送 {b_i} 给 Bob（经 1:1 MLS 会话）

Bob:    校验请求合理（限频、是否仍为联系人）
        for i: s'_i = b_i^d   mod N                                  # 盲签，Bob 看不到 t_i
        → 返回 {s'_i}

Alice:  for i: s_i = s'_i · r_i^{-1}   mod N   =   H(t_i ‖ ct_c ‖ epoch)^d
        存令牌  {(epoch, ct_c, t_i, s_i)}
```

要点：
- Bob **盲签**，事后无法把某 `t` 关联回签发会话（防 Bob/relay 合谋按签发批次画像）。
- 必须用 **RFC 9474 RSABSSA**（PSS + message randomizer），**不要自造**盲 RSA（防 one-more-forgery）。
- 令牌张数 `n` 由 Bob 决定并限频 ⇒ Alice 可发条数有界。Alice 用尽需再向 Bob 申请。

---

## 6. 兑付与 relay 验证 / Redemption & relay verification

```text
Alice → Relay:
    POST inbox_id, token=(epoch, ct_c, t, s), sealed_ct
        # sealed_ct = 对 Bob 加密的密文，内部封装【真实 sender 身份 + 正文/引用信封】

Relay 验证（全部离线、仅用链上公开数据）：
    1. epoch == CapabilityEpoch[Bob] ?                     // 新鲜度
    2. ct ∉ revoked_tags[inbox_id] ?                       // 定向撤销
    3. (N,e) = ipk_by_epoch[inbox_id][epoch]
       s^e mod N == H(t ‖ ct ‖ epoch) ?                    // RSABSSA 验签（不可伪造授权）
    4. t ∉ SpentSet[inbox_id] ?                            // 一次性（本 relay 视角）
    通过 ⇒ SpentSet[inbox_id].insert(t)；存 sealed_ct 入信箱；ACK
```

relay 学到：`inbox_id`、`ct`（联系人伪名）、随机 `t`、签名 `s`、密文长度/时间。
**学不到**：Alice 的账户身份、正文。Bob 拉取信箱、解密 `sealed_ct` 后才知发送方 = Alice。

---

## 7. 一次性语义与 k-超花降级 / One-time semantics & the k-over-spend downgrade

**根本张力**：`SpentSet` 是 **每 relay 各一份**。信箱 k-of-n 复制到多个 relay 时，Alice 把同一 `t`
投给不同 relay 即绕过单 relay 的一次性 —— 这是 e-cash 式双花。要「全局严格一次性」就需要 relay 间
对已花令牌达成共识 = 一条共享日志 = 等于上链 = 违背全链下初衷。

**定稿降级（已接受）**：

- **目标改为**：「**Bob 侧精确一次** + **relay 侧至多 k 份**」。
- **Bob 客户端按 `t` 去重**：同一 `t` 多副本到达只呈现一次。⇒ Bob 收到的 distinct 消息数 **≤ n**
  （Bob 签发的令牌总数），反垃圾上限严格成立。
- **relay 侧**：至多 k 份副本（= 复制因子），属**存储成本**而非用户可见垃圾；非攻击放大面
  （Alice 无法靠一张令牌产生超过 k 份）。
- **spent set 增长**：靠 **令牌内嵌 `epoch` + 可选 `expiry`** 做 TTL；epoch 轮换即可整表裁剪。

> 形式化界：对单个 `inbox_id`，攻击者在一个 epoch 内能令 Bob 看到的 distinct 消息 ≤ Bob 实际签发的
> 令牌数；relay 层总副本 ≤ (令牌数 × k)。两者都不随攻击者算力放大。

---

## 8. 撤销 / Revocation

| 场景 | 手段 | 代价 |
|---|---|---|
| 怀疑泄露 / 换设备 / 整批失效 | `bump_capability_epoch()` → 换 `IPK_e`，旧令牌全废 | 需向所有现存联系人重签 |
| 删除/拉黑**单个**联系人 c | 把 `ct_c` 加入 `revoked_tags[inbox_id]` | 该联系人流量被 relay 伪名暴露（见 §4 取舍）；其余不受影响 |

两者正交：epoch 是「全量核弹」，标签是「定点清除」。`CapabilityEpoch` 已在
`pallet-chat-permission` 落地，`revoked_tags` 待随 inbox 注册表落地。

---

## 9. 匿名性分析 / Anonymity analysis

各方在**诚实但好奇**模型下学到什么：

| 观察者 | 学到 | 学不到 |
|---|---|---|
| Relay | `inbox_id`、联系人伪名 `ct`、`t/s`、消息长度与时间 | 发送方账户、接收方账户（前提：注册表不可关联）、正文 |
| Bob（接收方） | 发送方真实身份（解密 `sealed_ct`）、`ct_c` | —（Bob 本就该知道谁在跟他聊） |
| 第三方（仅看链上） | `inbox_id` 存在、epoch、撤销表大小 | 谁拥有 inbox、谁与谁通信、消息内容 |

**匿名性上限 = inbox 注册表的不可关联性**：若 `inbox_id → IPK` 在链上可被关联回 Bob 的 AccountId，
则 relay 验签即泄露接收方身份。令牌方案与 inbox-id 方案**必须协同设计**。

**未由本协议解决（单列，需混淆层）**：relay 可见某 `inbox_id` / `ct` 的**流量时序与体量**，
可做相关性分析。对策（cover traffic / 批处理 / mixnet）属独立的传输隐私层，不在本规范内。

---

## 10. 安全陷阱 / Pitfalls（落地前必读）

1. **盲 RSA 实现**：严格按 **RFC 9474 RSABSSA**（含 message randomizer 抗 one-more-forgery）；
   `N ≥ 3072` 位；`e` 固定 65537；`H` 用 RFC 指定的 PSS/MGF1 流程。**禁止自造**。
2. **Bob 多端签发**：私钥 `d` 在 Bob 设备。多端需**门限 RSA** 或受控密钥同步；否则单端签发、其余端只发请求。
3. **跨 relay 双花**：已按 §7 降级处理；客户端**必须**实现按 `t` 去重，否则降级不成立。
4. **标签可链接性**：`ct_c` 长期稳定 ⇒ 同一联系人流量可伪名聚类。若需更强不可链接，升级 BBS+（§11）。
5. **spent set DoS**：无 TTL 会无限增长；令牌**必须**带 `epoch`（+可选 `expiry`），按 epoch 裁剪。
6. **令牌重绑攻击**：`ct/epoch` 必须**绑入签名消息** `H(t‖ct‖epoch)`，防止攻击者改 `ct/epoch` 复用 `s`。
7. **注册表可关联性**：见 §9 上限，inbox-id 注册不得泄露 AccountId。
8. **首次触达不在此通道**：陌生人首发走**公共自荐信箱**（PoW + per-inbox 配额），与联系人令牌互不混用
   （见 `CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md` 后续小节）。

---

## 11. 升级路径 / Upgrade path：BBS+ 匿名凭证

当需要**更强不可链接**（无需 relay 可见的稳定 `ct`）或**属性零知识**（如「epoch ≥ E」、群成员证明）
或**累加器撤销**时，用 BBS+ 替换 Blind-RSA：

- 凭证编码属性 `(ct, epoch, ...)`，兑付时**选择性披露 + 零知识谓词**；
- 撤销用**累加器**（非成员证明），定向撤销不再泄露稳定标签；
- 代价：配对运算、实现复杂度、证明体积更大。

基线（Blind-RSA）先上线、可验证、依赖少；BBS+ 作为隐私增强的二期。

---

## 12. 参数与体量 / Parameters & sizes

| 项 | 取值 / 估算 |
|---|---|
| RSA 模数 `N` | 3072 位（≈384B 公钥/签名） |
| `e` | 65537 |
| nonce `t` / 标签 `ct` | 各 32B |
| 单条消息令牌开销 | `epoch`(4B)+`ct`(32B)+`t`(32B)+`s`(384B) ≈ **452B** |
| relay 验签成本 | 一次小指数模幂（廉价） |
| 复制因子 `k` | 建议 3（与 storage-service Critical Tier 对齐） |
| 令牌批量 `n` | 由 Bob 限频策略定（如每联系人每 epoch ≤ 数百） |
| spent set TTL | 随 `epoch` 轮换裁剪；可叠加令牌内 `expiry` |

---

## 13. 落地清单 / Implementation checklist

1. **链上**：inbox 注册表 pallet —— ✅ **v1 已落地 `pallet-chat-inbox`**（runtime index 78）。
   `inbox_id → {controller, epoch, revoked_tags, deposit, created_at}`；IPK 下链（见 §3）；epoch 为
   inbox 维度（不复用 `CapabilityEpoch`）；v1 用签名 controller + 押金反垃圾（不可关联性靠一次性
   controller，账户无关注册为后续加固）。runtime API + RPC 已暴露只读校验面。单测 12/12 通过。
2. **relay 程序**：RFC 9474 验签、per-inbox `SpentSet`（带 TTL）、k-of-n 复制、ACK/重试、计费上报
   （挂 `pallet-storage-service`）。
3. **客户端**：盲化签发请求、令牌存储、兑付、**按 `t` 去重**、epoch/撤销感知、sealed-sender 封装。
4. **密码学评审**：RSABSSA 选型与参数、门限签发（多端）、与 inbox-id 不可关联性的联合证明。

---

## 14. 范围外 / Out of scope

- 首次触达（公共自荐信箱 / PoW）—— 见 `CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md`。
- 传输层时序/体量相关性的混淆（cover traffic / mixnet）。
- relay 网络的注册/质押/惩罚经济模型 —— 见链下投递总设计。
- 多端历史归档与恢复（加密归档 → IPFS）—— 见 `CHAT_LARGE_FILE_SPEC.md` 与通讯录保险库设计。
