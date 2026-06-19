// EN: Versioned AES-GCM blob wire format (ADR CHAT_SYNC_ANCHOR §5.0 migration rule).
//   v2 wire: 0x02 || iv(12) || ciphertext     — sealed under the vault_master-rooted key.
//   legacy : iv(12) || ciphertext             — sealed under the address-derived legacy key.
// Decode order: if the first byte is 0x02 try v2 first; on GCM auth failure fall back to
// the legacy parse (a legacy iv could start with 0x02 by chance). Writers emit v2 only.
// CN: 版本化 AES-GCM blob wire 格式（ADR CHAT_SYNC_ANCHOR §5.0 迁移规则）。
//   v2 wire：0x02 || iv(12) || 密文            ——用 vault_master 根派生钥封装。
//   旧格式 ：iv(12) || 密文                    ——用地址派生旧钥封装。
// 解析顺序：首字节为 0x02 时先按 v2 尝试；GCM 认证失败回退旧格式解析（旧 iv 可能恰好以
// 0x02 开头）。写入只产出 v2。

export const BLOB_WIRE_V2 = 0x02;

const IV_LEN = 12;

/// EN: Seal plaintext as `0x02 || iv || ct`. CN: 封装明文为 `0x02 || iv || ct`。
export async function sealVersionedBlob(key: CryptoKey, plaintext: Uint8Array): Promise<Uint8Array> {
  const iv = crypto.getRandomValues(new Uint8Array(IV_LEN));
  const ct = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: iv as BufferSource },
    key,
    plaintext as BufferSource,
  );
  const out = new Uint8Array(1 + IV_LEN + ct.byteLength);
  out[0] = BLOB_WIRE_V2;
  out.set(iv, 1);
  out.set(new Uint8Array(ct), 1 + IV_LEN);
  return out;
}

async function tryDecrypt(
  key: CryptoKey,
  iv: Uint8Array,
  ct: Uint8Array,
): Promise<Uint8Array | null> {
  try {
    const pt = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: iv as BufferSource },
      key,
      ct as BufferSource,
    );
    return new Uint8Array(pt);
  } catch (e) {
    if (e instanceof DOMException && e.name === "OperationError") return null;
    throw e;
  }
}

/// EN: Open a v2-or-legacy packed blob. `key` is the current (vault_master-rooted) key;
/// `legacyKey` the pre-§5.0 address-derived key (null when unavailable — then only v2 /
/// same-key legacy parses can succeed). Throws on authentication failure of all parses.
/// CN: 解析 v2 或旧格式 blob。`key` 为当前（vault_master 根）密钥；`legacyKey` 为 §5.0 前
/// 地址派生旧钥（不可用时为 null——此时仅 v2 / 同钥旧格式可解）。全部解析失败则抛错。
export async function openVersionedBlob(
  packed: Uint8Array,
  key: CryptoKey,
  legacyKey: CryptoKey | null,
): Promise<Uint8Array> {
  if (packed.length > 1 + IV_LEN && packed[0] === BLOB_WIRE_V2) {
    const pt = await tryDecrypt(key, packed.slice(1, 1 + IV_LEN), packed.slice(1 + IV_LEN));
    if (pt) return pt;
  }
  const iv = packed.slice(0, IV_LEN);
  const ct = packed.slice(IV_LEN);
  const pt = await tryDecrypt(legacyKey ?? key, iv, ct);
  if (pt) return pt;
  throw new Error("blobSeal: ciphertext does not authenticate under current or legacy key");
}
