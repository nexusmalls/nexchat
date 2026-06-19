// EN: E2EI device-leaf credential for Wire 1:1 multi-leaf (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC
// §3.9). The gap it closes: an MLS leaf identity is the opaque string `account#deviceFp` with NO
// cryptographic binding to the account's on-chain key, so a forged KeyPackage could claim any account.
// This binds a device to the account's SS58 key: the account key signs `(ctx ‖ account ‖ deviceId ‖
// leafSignatureKey)`. The signature rides INSIDE MLS as a custom leaf-node extension, so it travels with
// the leaf and stays valid across every KeyPackage the device mints. ANY holder of the SS58 address (a
// contact / 1:1 peer) can verify it WITHOUT trusting the relay — the end-to-end (relay-trustless) anchor
// that hardens peer-assisted Add (§3.8) and member-side re-verification beyond the relay's account-auth
// stamp. Pure + deterministic message builder → unit-testable; verification uses @polkadot/util-crypto
// and auto-detects the SS58 crypto. (The earlier v1 binding over one-time KeyPackage bytes — a separate
// request-level `cred` — was retired once every engine embeds this in-MLS binding.)
//
// CN: Wire 1:1 多 leaf 的 E2EI 设备 leaf 凭证（串行化规范 §3.9）。弥补的缺口：MLS leaf 身份是不透明字符串
// `account#deviceFp`，与账户链上密钥**无密码学绑定**，故伪造 KeyPackage 可冒充任意账户。本凭证把设备绑定到
// 账户 SS58 钥：账户钥签名 `(ctx ‖ account ‖ deviceId ‖ leafSignatureKey)`。签名作为自定义 leaf-node 扩展
// **驻留 MLS 内**，随 leaf 走、对该设备铸造的每个 KeyPackage 持续有效。任何持有该 SS58 地址者（联系人 / 1:1
// 对端）**无需信任 relay** 即可验证——这是把对端代 Add（§3.8）与成员侧复验硬化到超越 relay 账户盖章的端到端
// （relay-trustless）锚。纯函数、确定性消息构造 → 可单测；验证用 @polkadot/util-crypto 并自动识别 SS58 的
// 密码学类型。（早先对一次性 KeyPackage 字节的 v1 绑定——单发的请求级 `cred`——在全引擎嵌入本 MLS 内绑定后
// 已退役。）

/// EN: Signature domain-separation context (versioned): binds the account to the device's STABLE MLS leaf
/// signature key, so the credential rides INSIDE MLS as a leaf-node extension valid across every
/// KeyPackage the device mints. CN: 签名域分离上下文（带版本）：把账户绑定到设备**稳定的 MLS leaf 签名钥**，
/// 使凭证作为 leaf-node 扩展**驻留 MLS 内**、对该设备铸造的每个 KeyPackage 持续有效。
export const DEVICE_LEAF_KEY_CRED_CONTEXT = "nexus/chat-wire/device-leaf-key-cred/v1";

const enc = new TextEncoder();

function u64Le(value: number): Uint8Array {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(value), true);
  return out;
}

function concatBytes(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

/// EN: Canonical signing bytes binding an account device to its STABLE leaf signature key:
/// `ctx ‖ len(account)‖account ‖ len(deviceId)‖deviceId ‖ len(leafKey)‖leafKey`. This is the blob the
/// account SS58 key signs; the resulting signature rides inside MLS as a leaf-node extension (§3.9).
/// CN: 二阶段规范签名字节，把账户设备绑定到其**稳定 leaf 签名钥**：
/// `ctx ‖ 长度(account)‖account ‖ 长度(deviceId)‖deviceId ‖ 长度(leafKey)‖leafKey`。账户 SS58 钥签此 blob，
/// 所得签名作为 leaf-node 扩展随 MLS 内传（§3.9）。
export function leafKeyBindingBytes(
  account: string,
  deviceId: string,
  leafSignatureKey: Uint8Array,
): Uint8Array {
  const acct = enc.encode(account);
  const dev = enc.encode(deviceId);
  return concatBytes(
    enc.encode(DEVICE_LEAF_KEY_CRED_CONTEXT),
    u64Le(acct.length),
    acct,
    u64Le(dev.length),
    dev,
    u64Le(leafSignatureKey.length),
    leafSignatureKey,
  );
}

/// EN: Verify the in-MLS device-leaf credential: `sigHex` must be the account key's signature over
/// `leafKeyBindingBytes(account, deviceId, leafSignatureKey)`, proving this leaf key belongs to
/// `account`. Crypto type inferred from the SS58 `account`. Never throws. CN: 验证 MLS 内设备
/// leaf 凭证：`sigHex` 须为账户钥对 `leafKeyBindingBytes(...)` 的签名，证明该 leaf 钥属于 `account`。
/// 密码学类型由 SS58 `account` 推断。绝不抛错。
export async function verifyLeafKeyBinding(
  account: string,
  deviceId: string,
  leafSignatureKey: Uint8Array,
  sigHex: string,
): Promise<boolean> {
  try {
    const { signatureVerify, cryptoWaitReady } = await import("@polkadot/util-crypto");
    await cryptoWaitReady();
    const bytes = leafKeyBindingBytes(account, deviceId, leafSignatureKey);
    return signatureVerify(bytes, sigHex, account).isValid;
  } catch {
    return false;
  }
}
