// EN: MlsEngine — the ONLY place cryptography lives. Two implementations:
//   - WebCryptoMlsEngine: REAL AES-256-GCM over the P3 envelope, keyed by a
//     per-conversation key from KeyVault. Honest transport encryption standing in
//     for the MLS application-message layer until OpenMLS WASM is wired (Phase 1+).
//     The relay only ever sees ciphertext — the architectural invariant holds.
//   - StubMlsEngine: plaintext passthrough (tests / no-crypto fallback).
// CN: MlsEngine——密码学唯一所在。两个实现：WebCryptoMlsEngine 用 KeyVault 的会话密钥
// 对 P3 信封做真实 AES-256-GCM（MLS 应用消息层接入 OpenMLS WASM 前的传输加密占位，relay
// 只见密文）；StubMlsEngine 为明文直通（测试/无密码学回退）。

import { keyVault } from "@/keyvault/keyvault";
import {
  decodeEnvelope,
  encodeEnvelope,
  type EnvelopeV1,
} from "@/mls/envelope";

export type { EnvelopeV1 as DecryptedEnvelope } from "@/mls/envelope";

export interface MlsEngine {
  ensureKeyPackages(min: number): Promise<number>;
  encrypt(convId: string, plaintext: EnvelopeV1): Promise<Uint8Array>;
  decrypt(convId: string, ciphertext: Uint8Array): Promise<EnvelopeV1>;
  processWelcome(groupId: number, welcome: Uint8Array): Promise<void>;
}

const IV_LEN = 12;

/// EN: Real AES-256-GCM engine. Frame layout: [12B IV] ‖ [GCM ciphertext+tag].
/// CN: 真实 AES-256-GCM 引擎。帧布局：[12B IV] ‖ [GCM 密文+tag]。
export class WebCryptoMlsEngine implements MlsEngine {
  async ensureKeyPackages(min: number): Promise<number> {
    // EN: real client publishes KeyPackages on-chain; here it's a no-op count.
    // CN: 真实客户端会上链发布 KeyPackage；此处仅返回计数。
    return min;
  }

  async encrypt(convId: string, plaintext: EnvelopeV1): Promise<Uint8Array> {
    const key = await keyVault.deriveConvKey(convId);
    const iv = crypto.getRandomValues(new Uint8Array(IV_LEN));
    const ct = new Uint8Array(
      await crypto.subtle.encrypt(
        { name: "AES-GCM", iv: iv as BufferSource },
        key,
        encodeEnvelope(plaintext) as BufferSource,
      ),
    );
    const frame = new Uint8Array(IV_LEN + ct.length);
    frame.set(iv, 0);
    frame.set(ct, IV_LEN);
    return frame;
  }

  async decrypt(convId: string, frame: Uint8Array): Promise<EnvelopeV1> {
    const key = await keyVault.deriveConvKey(convId);
    const iv = frame.slice(0, IV_LEN);
    const ct = frame.slice(IV_LEN);
    const pt = new Uint8Array(
      await crypto.subtle.decrypt(
        { name: "AES-GCM", iv: iv as BufferSource },
        key,
        ct as BufferSource,
      ),
    );
    return decodeEnvelope(pt);
  }

  async processWelcome(): Promise<void> {
    // EN: wired with OpenMLS Welcome handling in a later phase. CN: 后续接 OpenMLS。
  }
}

/// EN: Plaintext passthrough (no crypto). CN: 明文直通（无密码学）。
export class StubMlsEngine implements MlsEngine {
  async ensureKeyPackages(min: number): Promise<number> {
    return min;
  }
  async encrypt(_convId: string, plaintext: EnvelopeV1): Promise<Uint8Array> {
    return encodeEnvelope(plaintext);
  }
  async decrypt(_convId: string, ciphertext: Uint8Array): Promise<EnvelopeV1> {
    return decodeEnvelope(ciphertext);
  }
  async processWelcome(): Promise<void> {
    /* stub */
  }
}

// EN: Prefer real crypto when WebCrypto subtle is available. CN: 有 WebCrypto 时用真加密。
const hasSubtle =
  typeof globalThis.crypto !== "undefined" && !!globalThis.crypto.subtle;

export const mlsEngine: MlsEngine = hasSubtle
  ? new WebCryptoMlsEngine()
  : new StubMlsEngine();
