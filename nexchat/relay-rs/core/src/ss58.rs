// EN: SS58 normalization to the generic prefix 42 — byte-for-byte parity with the Node relay
// (`relay-ss58.mjs`, `RPC_SS58 = 42` via @polkadot/util-crypto `encodeAddress(pk, 42)`). This
// is the relay's storage/lookup key for every account-keyed map (pointers, inboxes, mailboxes,
// endpoints), so it MUST match the Node relay's prefix exactly or pre-existing on-disk state
// becomes unreachable. SS58 = base58( prefix-bytes || pubkey || blake2b-512("SS58PRE"||data)[..2] ).
// `normalize_account` decodes any valid SS58 account (32-byte pubkey) and re-encodes it with
// prefix 42; malformed input is returned unchanged (parity with the JS try/catch).
// CN: SS58 规范化为通用前缀 42——与 Node relay（`relay-ss58.mjs`，`RPC_SS58 = 42`）逐字节一致。
// 这是 relay 所有账户键（指针/inbox/邮箱/端点）的存储与查询键，**必须**与 Node relay 前缀完全
// 一致，否则磁盘中既有状态将无法命中。非法输入原样返回（与 JS try/catch 一致）。
//
// NOTE: an earlier revision used prefix 273 (the Nexus chain prefix) per the design docs, but
// the authoritative drop-in target is the Node relay, which keys by 42. Compatibility wins over
// the docs here. / 注：早前版本按设计文档用 273（Nexus 链前缀），但 drop-in 的权威对象是
// Node relay（按 42 存键），兼容性优先于文档。

use blake2::{Blake2b512, Digest};

const PREFIX: u16 = 42;
const SS58PRE: &[u8] = b"SS58PRE";

fn checksum(payload: &[u8]) -> [u8; 2] {
    let mut h = Blake2b512::new();
    h.update(SS58PRE);
    h.update(payload);
    let out = h.finalize();
    [out[0], out[1]]
}

/// EN: Encode an SS58 address-type prefix into its 1- or 2-byte wire form.
/// CN: 把 SS58 地址类型前缀编码为 1 或 2 字节形式。
fn encode_prefix_bytes(ident: u16) -> Vec<u8> {
    if ident < 64 {
        vec![ident as u8]
    } else {
        // 14-bit ident split across two bytes (Substrate SS58 spec).
        let ident = ident & 0b0011_1111_1111_1111;
        let first = (((ident & 0b0000_0000_1111_1100) >> 2) as u8) | 0b0100_0000;
        let second = ((ident >> 8) as u8) | (((ident & 0b0000_0000_0000_0011) as u8) << 6);
        vec![first, second]
    }
}

/// EN: Decode an SS58 string to its 32-byte account id (verifies checksum). CN: 解码取 32B 公钥。
fn decode_account(addr: &str) -> Option<[u8; 32]> {
    let data = bs58::decode(addr).into_vec().ok()?;
    if data.len() < 3 {
        return None;
    }
    // Address-type prefix: 1 byte if <= 63, else 2 bytes (Substrate SS58 spec).
    let offset = if data[0] <= 63 {
        1usize
    } else if data[0] < 128 {
        if data.len() < 4 {
            return None;
        }
        2usize
    } else {
        return None;
    };
    if data.len() != offset + 32 + 2 {
        return None; // only AccountId32 addresses are normalized
    }
    let body = &data[..offset + 32];
    let want = checksum(body);
    if want != [data[offset + 32], data[offset + 33]] {
        return None;
    }
    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&data[offset..offset + 32]);
    Some(pubkey)
}

/// EN: Encode a 32-byte account id as SS58 with the generic prefix 42. CN: 用前缀 42 编码 32B 公钥。
pub fn encode_account_id(pubkey: &[u8; 32]) -> String {
    encode_prefix42(pubkey)
}

/// EN: Decode SS58 to 32-byte account id; None if invalid. CN: SS58 解码为 32B 账户 id。
pub fn account_pubkey(addr: &str) -> Option<[u8; 32]> {
    decode_account(addr)
}

fn encode_prefix42(pubkey: &[u8; 32]) -> String {
    let prefix = encode_prefix_bytes(PREFIX);
    let mut payload = Vec::with_capacity(prefix.len() + 32 + 2);
    payload.extend_from_slice(&prefix);
    payload.extend_from_slice(pubkey);
    let cs = checksum(&payload);
    payload.extend_from_slice(&cs);
    bs58::encode(payload).into_string()
}

/// EN: Normalize any SS58 account to prefix 42 (Node-relay parity); return input unchanged if
/// not a valid AccountId32 SS58 string. CN: 规范化到前缀 42（与 Node relay 一致）；非法输入原样返回。
pub fn normalize_account(addr: &str) -> String {
    match decode_account(addr) {
        Some(pk) => encode_prefix42(&pk),
        None => addr.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Alice's well-known sr25519 account, encoded with the Nexus prefix 273.
    const ALICE_273: &str = "X4Y9wZky3HPgyUGy5xH1RrwEVg3rTuzxYQ1GAKWscgAysZvxT";
    // Same account encoded with the substrate generic prefix 42.
    const ALICE_42: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
    // Alice's raw 32-byte account id.
    const ALICE_PK: [u8; 32] = [
        0xd4, 0x35, 0x93, 0xc7, 0x15, 0xfd, 0xd3, 0x1c, 0x61, 0x14, 0x1a, 0xbd, 0x04, 0xa9, 0x9f,
        0xd6, 0x82, 0x2c, 0x85, 0x58, 0x85, 0x4c, 0xcd, 0xe3, 0x9a, 0x56, 0x84, 0xe7, 0xa5, 0x6d,
        0xa2, 0x7d,
    ];

    #[test]
    fn decodes_known_alice_account() {
        assert_eq!(decode_account(ALICE_273).unwrap(), ALICE_PK);
        assert_eq!(decode_account(ALICE_42).unwrap(), ALICE_PK);
    }

    #[test]
    fn encodes_alice_to_42_golden() {
        // Matches @polkadot/util-crypto encodeAddress(pk, 42) — the Node relay's RPC_SS58.
        assert_eq!(encode_prefix42(&ALICE_PK), ALICE_42);
    }

    #[test]
    fn prefix42_is_idempotent() {
        assert_eq!(normalize_account(ALICE_42), ALICE_42);
    }

    #[test]
    fn cross_prefix_normalizes_to_42() {
        // The Nexus prefix-273 encoding must normalize to the generic prefix 42 (Node relay key).
        assert_ne!(ALICE_42, ALICE_273);
        assert_eq!(normalize_account(ALICE_273), ALICE_42);
    }

    #[test]
    fn invalid_returned_unchanged() {
        assert_eq!(normalize_account("not-an-address"), "not-an-address");
        assert_eq!(normalize_account(""), "");
    }

    #[test]
    fn random_pubkey_round_trips() {
        let pk = [7u8; 32];
        let addr = encode_prefix42(&pk);
        assert_eq!(decode_account(&addr).unwrap(), pk);
        assert_eq!(normalize_account(&addr), addr);
    }
}
