// EN: Envelope codec + real AES-GCM engine round-trip tests.
// CN: 信封编解码 + 真实 AES-GCM 引擎往返测试。

import { describe, it, expect } from "vitest";
import {
  encodeEnvelope,
  decodeEnvelope,
  textEnvelope,
  stampEnvelopeSentAt,
  envelopeSentAt,
  type EnvelopeV1,
} from "./envelope";
import { WebCryptoMlsEngine } from "./mlsEngine";
import { keyVault } from "@/keyvault/keyvault";

describe("P3 envelope codec", () => {
  it("round-trips a text envelope with P3 fields", () => {
    const env = textEnvelope("m1", "hello", {
      replyTo: "m0",
      mentions: ["alice"],
      ephemeralMs: 60000,
    });
    const back = decodeEnvelope(encodeEnvelope(env));
    expect(back.id).toBe("m1");
    expect(back.type).toBe("text");
    expect((back.body as { text: string }).text).toBe("hello");
    expect(back.replyTo).toBe("m0");
    expect(back.mentions).toEqual(["alice"]);
    expect(back.ephemeral?.ttlMs).toBe(60000);
  });

  it("preserves optional sentAt through encode/decode", () => {
    const env = stampEnvelopeSentAt(textEnvelope("m2", "hi", {}), 1_700_000_000_000);
    const back = decodeEnvelope(encodeEnvelope(env));
    expect(back.sentAt).toBe(1_700_000_000_000);
    expect(envelopeSentAt(back)).toBe(1_700_000_000_000);
  });

  it("rejects unknown version", () => {
    const bad = new TextEncoder().encode(JSON.stringify({ v: 2, id: "x" }));
    expect(() => decodeEnvelope(bad)).toThrow(/version/);
  });
});

describe("WebCryptoMlsEngine (AES-256-GCM)", () => {
  it("encrypts then decrypts to the same envelope", async () => {
    keyVault.initForTest("test-seed");
    const engine = new WebCryptoMlsEngine();
    const convId = "d:5Bob...xyz";
    const env = textEnvelope("m42", "secret message", {});
    const frame = await engine.encrypt(convId, env);

    // ciphertext must NOT contain the plaintext (relay sees only ciphertext)
    const asText = new TextDecoder().decode(frame);
    expect(asText.includes("secret message")).toBe(false);

    const back = await engine.decrypt(convId, frame);
    expect((back.body as { text: string }).text).toBe("secret message");
  });

  it("fails to decrypt under a different conversation key", async () => {
    keyVault.initForTest("test-seed");
    const engine = new WebCryptoMlsEngine();
    const env: EnvelopeV1 = textEnvelope("m1", "x", {});
    const frame = await engine.encrypt("d:a", env);
    await expect(engine.decrypt("d:b", frame)).rejects.toBeDefined();
  });
});
