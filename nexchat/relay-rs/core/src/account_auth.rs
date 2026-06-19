// EN: Account-binding signatures for relay `register_account` (sr25519 / Substrate `substrate`
// signing context, parity with `@polkadot/keyring` `pair.sign(bytes)`). Used when
// `RELAY_STRICT_AUTH=1`.
// CN: relay `register_account` 的账户绑定签名（sr25519 / Substrate `substrate` 签名上下文，与
// `@polkadot/keyring` `pair.sign(bytes)` 一致）。在 `RELAY_STRICT_AUTH=1` 时启用。

use crate::ss58::account_pubkey;

/// EN: Canonical v1 payload: `nexchat-relay-register-v1\\0` + endpointId + `\\0` + account (SS58-42).
/// CN: 规范 v1 载荷：`nexchat-relay-register-v1\\0` + endpointId + `\\0` + account（SS58-42）。
pub fn register_account_sign_payload(endpoint_id: &str, account_normalized: &str) -> Vec<u8> {
    let mut out = b"nexchat-relay-register-v1\0".to_vec();
    out.extend_from_slice(endpoint_id.as_bytes());
    out.push(0);
    out.extend_from_slice(account_normalized.as_bytes());
    out
}

/// EN: Verify sr25519 signature over raw bytes (`substrate` signing context — parity with
/// `@polkadot/keyring` `pair.sign(bytes)` / wasm `ext_sr25519_sign`). CN: 验证裸字节上的 sr25519
/// 签名（`substrate` 签名上下文——与 `@polkadot/keyring` `pair.sign(bytes)` / wasm
/// `ext_sr25519_sign` 一致）。
pub fn verify_sr25519_raw(pubkey: &[u8; 32], message: &[u8], sig64: &[u8]) -> bool {
    if sig64.len() != 64 {
        return false;
    }
    let Ok(pk) = schnorrkel::PublicKey::from_bytes(pubkey) else {
        return false;
    };
    let Ok(sig) = schnorrkel::Signature::from_bytes(sig64) else {
        return false;
    };
    pk.verify_simple(b"substrate", message, &sig).is_ok()
}

/// EN: Verify `register_account` binding for normalized SS58-42 `account`. CN: 验证 `register_account` 绑定。
pub fn verify_register_account_sig(
    endpoint_id: &str,
    account_normalized: &str,
    sig64: &[u8],
) -> bool {
    let Some(pk) = account_pubkey(account_normalized) else {
        return false;
    };
    let msg = register_account_sign_payload(endpoint_id, account_normalized);
    verify_sr25519_raw(&pk, &msg, sig64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ss58::{encode_account_id, normalize_account};
    use schnorrkel::Keypair;

    #[test]
    fn register_sig_roundtrip() {
        let kp = Keypair::generate();
        let pk_bytes = kp.public.to_bytes();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&pk_bytes);
        let account = normalize_account(&encode_account_id(&arr));
        let payload = register_account_sign_payload("dev1", &account);
        let sig = kp.sign_simple(b"substrate", payload.as_slice()).to_bytes();
        assert!(verify_register_account_sig("dev1", &account, &sig));
        assert!(!verify_register_account_sig("other", &account, &sig));
    }

    /// EN: `@polkadot/keyring` `pair.sign` over register payload (//Alice, ss58 273 wire / 42 norm).
    /// CN: `@polkadot/keyring` `pair.sign` 对 register 载荷的签名（//Alice，273 线格式 / 42 规范化）。
    #[test]
    fn node_polkadot_keyring_sig_compat() {
        const ALICE_NORM: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
        const ENDPOINT: &str = "probe-auth-273";
        let sig: [u8; 64] = [
            0x46, 0x4c, 0x83, 0xd0, 0x6a, 0xe7, 0x8a, 0x45, 0x12, 0xfc, 0xdb, 0x99, 0xad, 0x24,
            0x43, 0x21, 0x66, 0xf2, 0x19, 0x07, 0xff, 0x74, 0x97, 0x23, 0x02, 0x99, 0xeb, 0xa4,
            0xc9, 0x4e, 0x5d, 0x30, 0x46, 0x0e, 0xab, 0xaa, 0x7f, 0x09, 0x2a, 0xe7, 0x2f, 0xa5,
            0x36, 0x7f, 0xae, 0xdd, 0x77, 0x68, 0xde, 0x0f, 0xb8, 0x48, 0x4a, 0xf1, 0xbf, 0x6b,
            0xfd, 0xc6, 0x15, 0xde, 0x9a, 0x45, 0x18, 0x86,
        ];
        assert!(verify_register_account_sig(ENDPOINT, ALICE_NORM, &sig));
    }
}
