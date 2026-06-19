// EN: Tests for the Track A device peer key + sealed-box (design §5.1/§5.2): keygen, directory-key
// endorsement verify/tamper, and the anonymous-sender ECIES seal/open (round-trip + wrong-recipient +
// tamper rejection). Crypto is REAL (WebCrypto ECDH P-256 + polkadot ed25519). CN: 路线 A 设备对端钥 +
// 封装盒（设计 §5.1/§5.2）单测：密钥生成、目录钥背书验证/篡改、匿名发送方 ECIES 封装/解封（往返 + 收件人不符
// + 篡改拒绝）。密码学为真（WebCrypto ECDH P-256 + polkadot ed25519）。

import { describe, expect, it } from "vitest";

import {
  endorseDevicePeerKey,
  generateDevicePeerKey,
  openSealed,
  sealToPeer,
  verifyDevicePeerEndorsement,
} from "@/mls/devicePeerKey";
import { deriveDeviceDirectoryKey } from "@/mls/sendingAuthority";

describe("device peer key endorsement (§5.1)", () => {
  it("verifies a directory-key endorsement and rejects tamper / wrong key", async () => {
    const dir = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(11));
    const other = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(12));
    const peer = await generateDevicePeerKey();

    const e = await endorseDevicePeerKey(dir, "devNew", peer.publicKeyRaw);
    expect(await verifyDevicePeerEndorsement(dir.publicKey, e)).toBe(true);
    // tampered device id → reject.
    expect(await verifyDevicePeerEndorsement(dir.publicKey, { ...e, deviceId: "devEvil" })).toBe(false);
    // wrong directory key → reject.
    expect(await verifyDevicePeerEndorsement(other.publicKey, e)).toBe(false);
  });
});

describe("sealed-box seal/open (§5.2)", () => {
  it("round-trips a bundle to the intended recipient", async () => {
    const recipient = await generateDevicePeerKey();
    const bundle = crypto.getRandomValues(new Uint8Array(200));

    const sealed = await sealToPeer(recipient.publicKeyRaw, bundle);
    // ephemeral_pub(65) + iv(12) + ct(>=bundle+tag) — never the raw bundle.
    expect(sealed.length).toBeGreaterThan(65 + 12 + bundle.length);
    const opened = await openSealed(recipient.privateKey, sealed);
    expect(opened).toEqual(bundle);
  });

  it("a different device cannot open the sealed bundle", async () => {
    const recipient = await generateDevicePeerKey();
    const intruder = await generateDevicePeerKey();
    const sealed = await sealToPeer(recipient.publicKeyRaw, new Uint8Array([1, 2, 3, 4]));
    expect(await openSealed(intruder.privateKey, sealed)).toBeNull();
  });

  it("rejects a tampered / truncated ciphertext", async () => {
    const recipient = await generateDevicePeerKey();
    const sealed = await sealToPeer(recipient.publicKeyRaw, new Uint8Array([9, 9, 9]));
    const tampered = sealed.slice();
    tampered[tampered.length - 1] ^= 0xff;
    expect(await openSealed(recipient.privateKey, tampered)).toBeNull();
    expect(await openSealed(recipient.privateKey, sealed.slice(0, 80))).toBeNull();
  });
});
