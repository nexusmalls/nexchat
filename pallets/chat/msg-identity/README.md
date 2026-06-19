# Pallet Msg Identity

消息身份预密钥锚 pallet（crate 名 `pallet-msg-identity`）。已在 runtime 注册为 `MsgIdentity`
（pallet index **80**）。

X3DH + Double Ratchet 1:1 栈的链上 **Authentication Service** 锚：按 `(AccountId, DeviceId)`
托管 IK / SPK / OPK Merkle 根、设备级 `prekey_epoch` 与 1:1 栈能力位。链上**不存共享秘密、
不存棘轮态、不存明文**，不做 DH/AEAD。

设计文档：[`CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md`](../../CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md)
（§17 背书、§20 栈协商 `chooseStack`）。

> 与 pairwise MLS-Wire 1:1 栈**并行共存**；`ChatStackCaps` 公告双方能力，发起方选 DR 或回退 Wire。

## 1. 定位与边界

| 层 | 职责 |
| --- | --- |
| **链上** | 每设备 X25519 IK + 背书、SPK + 背书、OPK Merkle 根（叶子链下）、`prekey_epoch`、每账户 `StackCaps` |
| **链下** | OPK 叶子分发（relay）、X3DH 握手、Double Ratchet 会话态、MLS-Wire 会话 |
| **链不做** | 共享秘密、ratchet 状态、消息明文、背书密码学复验（见 §2） |

与 `pallet-chat-inbox` 正交：inbox 锚盲化投递令牌；本 pallet 锚 X3DH 预密钥与栈协商。

## 2. 背书边界（v1）

- **链上写入授权**：`ensure_signed`——仅账户本人可写自己的 `(account, device)` 子树。
- **背书存储**：`ik_endorsement` / `spk_endorsement` 为 **64 字节不透明**数据，链**不**对
  AccountId 复验签名。
- **链下消费方**（对端、relay）做 *relay-trustless* 校验：账户 sr25519 对冻结上下文签名：
  - IK：`CTX_IK_ENDORSE ‖ ik`（`nexchat/x3dh/ik-endorse/v1`）
  - SPK：`CTX_SPK_ENDORSE ‖ spk`（`nexchat/x3dh/spk-endorse/v1`）

## 3. 设备 ID 自证

`DeviceId = blake2_128(ik)`（16 字节）。注册时链校验 `device_id == blake2_128(ik)`（`DeviceIdMismatch`），
防止伪造设备路由。

## 4. Extrinsic 一览

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 0 | `register_device(device_id, ik, ik_endorsement)` | 注册 IK；预留 `DeviceDeposit`；`prekey_epoch` 从 0 |
| 1 | `set_signed_prekey(device_id, spk, spk_endorsement, valid_until)` | 设置/轮换 SPK（设备须已注册） |
| 2 | `set_opk_root(device_id, root, count)` | 发布/更新 OPK Merkle 根；`count > 0`；每次递增 OPK `epoch` |
| 3 | `bump_prekey_epoch(device_id)` | 设备级撤销纪元 +1；作废此前预密钥包 |
| 4 | `unregister_device(device_id)` | 注销设备、清空预密钥状态、退还押金 |
| 5 | `set_stack_caps(flags, version)` | 设置本账户 1:1 栈能力公告（§20） |
| 6 | `force_unregister_device(account, device_id)` | 治理强制注销；仅 `ForceOrigin` |

### `StackCaps.flags`

| 位 | 常量 | 含义 |
| --- | --- | --- |
| `0b0000_0001` | `STACK_DR` | 支持 X3DH + Double Ratchet |
| `0b0000_0010` | `STACK_MLS_WIRE` | 支持 pairwise MLS-Wire 1:1 |

发起方读双方 `stack_caps` 后按设计 §20 `chooseStack` 协商（双方支持 DR 则 DR 优先）。

## 5. 存储

| 存储 | 说明 |
| --- | --- |
| `DeviceIdentities` | `(account, device_id) → DeviceIdentity`（IK、背书、`prekey_epoch`、押金） |
| `DeviceSignedPreKeys` | `(account, device_id) → SignedPreKey` |
| `DeviceOpkRoots` | `(account, device_id) → OpkRoot`（root / count / epoch） |
| `DeviceCountByAccount` | 每账户已注册设备数 |
| `ChatStackCaps` | `account → StackCaps`（每账户一条） |

## 6. 配置（`Config`）— runtime 当前值

| 项 | runtime 值 |
| --- | --- |
| `DeviceDeposit` | 0.5 NEX（`UNIT / 2`，与 inbox 同级） |
| `MaxDevicesPerAccount` | 16 |
| `ForceOrigin` | Root 或技术委员会多数 |

## 7. Runtime API

`runtime_api::MsgIdentityApi`（只读、免费；经 `state_call` / `api.call` 调用）：

| 方法 | 说明 |
| --- | --- |
| `device_ik(account, device_id)` | `(ik, prekey_epoch)` 或 `None` |
| `device_spk(account, device_id)` | `(spk, valid_until)` 或 `None` |
| `device_opk_root(account, device_id)` | `(root, count, epoch)` 或 `None` |
| `stack_caps(account)` | `(flags, version)` 或 `None` |
| `device_exists(account, device_id)` | 是否已注册 |

> 当前 **无** 专用 `chat_*` JSON-RPC 封装（node 未挂载）；客户端用 polkadot-js `api.call.msgIdentityApi.*`
> 或 `state_call` 即可。

## 8. 事件与错误

**事件：** `DeviceRegistered` / `SignedPreKeySet` / `OpkRootSet` / `PrekeyEpochBumped` /
`DeviceUnregistered` / `DeviceForceUnregistered` / `StackCapsSet`

**错误：** `DeviceAlreadyExists` / `DeviceNotFound` / `DeviceIdMismatch` / `TooManyDevices` /
`EmptyOpkSet`

## 9. 依赖关系

```
pallet-chat-common  ←── pallet-msg-identity（bump_u32_epoch / next_u32_epoch / deposit 薄封装）
sp-io               ←── blake2_128(device_id 自证)
```

不依赖 `core` / `group` / `permission` / `inbox`。

## 10. 权重与基准

全 7 个 extrinsic 有 `WeightInfo` + `benchmarking.rs`；权重 dev 链实测。已加入 runtime 基准清单。
主网前应在参考硬件重跑 `runtime-benchmarks`。

## 11. X3DH 消费流程（客户端）

1. 读对端 `stack_caps` → `chooseStack`。
2. 枚举对端 `device_id`（链上或客户端已知设备列表）。
3. 对每个设备：`device_ik` + `device_spk` + `device_opk_root`；校验 `prekey_epoch` 新鲜度。
4. 链下验证 `ik_endorsement` / `spk_endorsement`（sr25519 + 冻结 CTX）。
5. 从 relay 取 OPK 叶子（Merkle 证明），运行 X3DH，建立 DR 会话。

## 12. 上线审计摘要（2026-06-19）

| 维度 | 结论 |
| --- | --- |
| **职责边界** | ✅ 仅 AS 锚；无秘密/棘轮/明文 |
| **设备自证** | ✅ `device_id = blake2_128(ik)` 链上强制 |
| **撤销模型** | ✅ `prekey_epoch` bump + OPK root epoch 递增 |
| **栈协商** | ✅ `StackCaps` + 冻结 flag 常量 |
| **背书模型** | ✅ 不透明存储 + 链下 relay-trustless 验签（v1 设计既定） |
| **反垃圾** | ✅ 每设备押金 + 账户设备上限 16 |
| **治理回收** | ✅ `force_unregister_device` |
| **Runtime 接线** | ✅ index 80 + `MsgIdentityApi` |
| **权重 / 基准** | ✅ 全 extrinsic benchmark |
| **单测** | ✅ 16 项通过（`cargo test -p pallet-msg-identity`） |
| **缺口（非阻塞）** | ⚪ 无 `chat_*` JSON-RPC（可用 `api.call`）；⚪ 链上不复验背书（v1 既定）；⚪ 主网前重跑 benchmark |

**总评：达到上线标准。** X3DH 预密钥锚、设备自证、撤销与栈协商能力已落地并有测试；
OPK 叶子与 X3DH 运行时在设计文档与 nexchat 客户端侧完成，不构成本 pallet 阻塞项。
