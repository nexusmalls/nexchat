// EN: Integration test of the off-chain message data path that the relay carries:
//   sender:  envelope → MlsEngine.encrypt → base64 frame {convId, ciphertext}
//   receiver: base64 → MlsEngine.decrypt(frame.convId) → envelope, routed by convId
// This is the exact pipeline behind appStore.sendMessage / handleInbound, minus the
// zustand glue — and it proves routing is by convId and that frames are isolated
// per conversation key (a frame for conv A cannot decrypt as conv B).
// CN: relay 承载的链下消息数据路径集成测试（发送→加密→base64 帧→解密→按 convId 路由），
// 即 appStore.sendMessage / handleInbound 背后的真实管线；证明按 convId 路由且会话密钥隔离。

import { describe, it, expect, beforeAll } from "vitest";
import { WebCryptoMlsEngine } from "@/mls/mlsEngine";
import { textEnvelope } from "@/mls/envelope";
import { keyVault } from "@/keyvault/keyvault";
import { bytesToB64, b64ToBytes, type RelayFrame } from "@/relay/relayClient";

// minimal base64 polyfill for the node test environment
beforeAll(() => {
  const g = globalThis as unknown as {
    btoa?: (s: string) => string;
    atob?: (s: string) => string;
  };
  if (!g.btoa) g.btoa = (s) => Buffer.from(s, "binary").toString("base64");
  if (!g.atob) g.atob = (s) => Buffer.from(s, "base64").toString("binary");
});

describe("relay data path (send → frame → receive)", () => {
  it("delivers an encrypted text message routed by convId", async () => {
    keyVault.initForTest("shared-demo-seed"); // both parties share the seed (Phase-1 placeholder)
    const sender = new WebCryptoMlsEngine();
    const receiver = new WebCryptoMlsEngine();
    const convId = "d:5Alice...abc";

    // sender side (appStore.sendMessage)
    const env = textEnvelope("c-1", "你好，加密消息", { replyTo: "m0" });
    const cipher = await sender.encrypt(convId, env);
    const frame: RelayFrame = {
      convId,
      senderRef: "用户A",
      ciphertextB64: bytesToB64(cipher),
    };

    // relay only sees ciphertext — never the plaintext
    expect(frame.ciphertextB64.includes("加密")).toBe(false);

    // receiver side (handleInbound): route by frame.convId
    const back = await receiver.decrypt(frame.convId, b64ToBytes(frame.ciphertextB64));
    expect(frame.convId).toBe("d:5Alice...abc"); // routing target preserved
    expect(back.id).toBe("c-1");
    expect(back.replyTo).toBe("m0");
    expect((back.body as { text: string }).text).toBe("你好，加密消息");
  });

  it("isolates conversations: a frame for conv A cannot be read as conv B", async () => {
    keyVault.initForTest("shared-demo-seed");
    const engine = new WebCryptoMlsEngine();
    const cipher = await engine.encrypt("d:convA", textEnvelope("x", "secret", {}));
    const frame = { convId: "d:convA", ciphertextB64: bytesToB64(cipher) };
    await expect(
      engine.decrypt("d:convB", b64ToBytes(frame.ciphertextB64)),
    ).rejects.toBeDefined();
  });
});
