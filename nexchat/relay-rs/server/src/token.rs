// EN: RFC 9474 blind delivery token verification. A finalized RSABSSA signature is a plain
// RSASSA-PSS signature over the *prepared* message `p` (32-byte randomizer ‖ t ‖ ct ‖ epoch),
// so verification == RSASSA-PSS-VERIFY(pk, p, s) with SHA-384 and salt_len = 48 (hLen). This
// matches @cloudflare/blindrsa-ts `RSABSSA.SHA384.PSS.Randomized().verify(pk, s, p)` used by
// relay-token-verify.mjs. The relay never re-prepares: `p` already carries the randomizer.
// CN: RFC 9474 盲签投递令牌验签。unblind 后的 RSABSSA 签名就是对 prepared 消息 `p`
// （32 字节随机前缀 ‖ t ‖ ct ‖ epoch）的标准 RSASSA-PSS 签名，故验签即
// RSASSA-PSS-VERIFY(pk, p, s)，SHA-384、salt_len=48。与 relay-token-verify.mjs 用的
// @cloudflare/blindrsa-ts 行为一致；relay 不再 re-prepare，`p` 已含随机前缀。

use base64::Engine;
use rsa::pss::{Signature, VerifyingKey};
use rsa::signature::Verifier;
use rsa::{BigUint, RsaPublicKey};
use sha2::Sha384;

/// EN: Inputs for one delivery-token verification (all base64). CN: 一次令牌验签的输入（均为 base64）。
pub struct DeliveryToken<'a> {
    pub ipk_n: &'a str,
    pub ipk_e: &'a str,
    pub s: &'a str,
    pub p: &'a str,
}

/// EN: Tolerant base64 decode accepting both standard (`+/`) and url-safe (`-_`) alphabets,
/// with or without padding — JWK `n`/`e` are base64url, `s`/`p` are standard base64.
/// CN: 宽松 base64 解码，兼容标准与 url-safe 字母表、有无填充——JWK n/e 为 base64url，s/p 为标准 base64。
fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let normalized: String = s
        .chars()
        .filter_map(|c| match c {
            '-' => Some('+'),
            '_' => Some('/'),
            '=' | '\r' | '\n' => None,
            other => Some(other),
        })
        .collect();
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(normalized)
        .ok()
}

/// EN: Verify a delivery token; returns false on any decode/key/signature error (parity with
/// the JS try/catch → false). CN: 验签；任何解码/构造/签名错误均返回 false（与 JS 一致）。
pub fn verify_delivery_token(tok: &DeliveryToken) -> bool {
    let (Some(n), Some(e), Some(sig_bytes), Some(msg)) = (
        b64_decode(tok.ipk_n),
        b64_decode(tok.ipk_e),
        b64_decode(tok.s),
        b64_decode(tok.p),
    ) else {
        return false;
    };
    let n = BigUint::from_bytes_be(&n);
    let e = BigUint::from_bytes_be(&e);
    let Ok(pk) = RsaPublicKey::new(n, e) else {
        return false;
    };
    let vk = VerifyingKey::<Sha384>::new(pk);
    let Ok(sig) = Signature::try_from(sig_bytes.as_slice()) else {
        return false;
    };
    vk.verify(&msg, &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known-answer vector generated from @cloudflare/blindrsa-ts RSABSSA.SHA384.PSS.Randomized:
    // t = 0x07*32, ct = 0x09*32, epoch = 3; full blind→sign→finalize→verify==true round-trip.
    const IPK_N: &str = "vKnD6MFkYbqJkEty4J0opYzUb9HXFsvCVihhUW3PERsp-trE-xoGLXH3hJCHRIqh3S9rsG9_qWd2KQgflSyTzeFwc1CWFxm3L1QJcS0MluJIRISGjrMH5eF2r7Ni28xPy6iYGX4lFZtZGntHKH1rZxgeUB7BnASmBuMrqVfzCupIAyoN0mpLl8jWK11m-IA9KuyzakuMpW3shDn_fpPIZEmnrHvsBgMbVFZ9rdDWoEEkMtTZOV2tXaTWqMgIURCI_kPo6JUa2XQV2XsMNSmNJmyjJWf-c1pTD1cGiaXoALsvxa9GFroRzK0TePMmazkZMcR9-QYW6rWoyW0MTyEqw-_TIduNG83hPWlEmS1H6KfVx_qgC5DslCA3oCrkzgMKB6fejUEe-65XsZsdPk9wZYVwyWb54cnawD5izqmtQ2qU7wvSZrZOuSGq2hCmad-HHhY0sf1EFH9busLeq-chKnyW4qsoz6CKom51Bq2Bbh4JxrbGC8o8LBD0r0dmKg67";
    const IPK_E: &str = "AQAB";
    const SIG: &str = "hD+nqC92td7VGlxZ3uvmv2+dIWGDKbv9IA7FyN9UOrFBPuD5/YOGcjmgEuWZL7Zw5Vwbr35uuT74kTtZghVUNrXgDu0OYAmO5h3iyB79mQPR+r0fiuTzIfPHzMBaPDjMohQpjcMLj6OSB2sTonG6ixD8iE+0rdYXgtiVTA7QHM+myxws0ovepOS9KCbIQLQSZADNAtrrBnjAlyrGqwd/LbKl8D+10GaS/wvQxoEfYkZeLU6LbxLpJQMW42HE1ftEoXVmL2ZNwFg4L4USlXY3BVJlHo/TUQdY+LUmyP6SHLL655bnOjl3pmjWLFJt5EFZLStqg/1YGd9xxaMIWze94qDJ8k0caWP5BF/GyNRbIGkU520V2vtprIkWH+r0JYQaqfLnU/6th7KOLBHyCMMwTlpsMKEOpyzX6FTdoEVYZhZgU4ifFSMOFYvmltevxliIcIrKzgm7DmgyZTzOV9O4PyPCCYJfV0wjxjRhYwnKqlLJL8mIkmaPntJ0SpVHD9Jr";
    const PREPARED: &str =
        "uHhVEgXGTGXY10agz09N29NJcoyoVwUyy7wBlkAgJRYHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJAAAAAw==";

    #[test]
    fn verifies_known_answer_vector() {
        assert!(verify_delivery_token(&DeliveryToken {
            ipk_n: IPK_N,
            ipk_e: IPK_E,
            s: SIG,
            p: PREPARED,
        }));
    }

    #[test]
    fn rejects_tampered_prepared_message() {
        // Flip the last epoch byte (epoch 3 -> 4) — signature must no longer verify.
        let mut p = base64::engine::general_purpose::STANDARD
            .decode(PREPARED)
            .unwrap();
        let last = p.len() - 1;
        p[last] = 4;
        let tampered = base64::engine::general_purpose::STANDARD.encode(&p);
        assert!(!verify_delivery_token(&DeliveryToken {
            ipk_n: IPK_N,
            ipk_e: IPK_E,
            s: SIG,
            p: &tampered,
        }));
    }

    #[test]
    fn rejects_malformed_inputs() {
        assert!(!verify_delivery_token(&DeliveryToken {
            ipk_n: "not base64 !!!",
            ipk_e: IPK_E,
            s: SIG,
            p: PREPARED,
        }));
        assert!(!verify_delivery_token(&DeliveryToken {
            ipk_n: IPK_N,
            ipk_e: IPK_E,
            s: "",
            p: PREPARED,
        }));
    }
}
