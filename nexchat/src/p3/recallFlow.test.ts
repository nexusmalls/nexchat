// EN: Recall control-path integration — sender encrypts `type=recall`, receiver decrypts and
// applies the shared handler (`applyRecallEnvelope`). Mirrors appStore recallMessage / inbound
// without zustand or a live relay.
// CN: 撤回控制路径集成——发送方加密 `type=recall`，接收方解密并走共享处理器；对应 appStore
// recallMessage / 入站分支，无需 zustand 或真实 relay。

import { beforeAll, describe, expect, it } from "vitest";
import { keyVault } from "@/keyvault/keyvault";
import { recallEnvelope, textEnvelope } from "@/mls/envelope";
import { WebCryptoMlsEngine } from "@/mls/mlsEngine";
import {
  applyRecallEnvelope,
  canRecallMessage,
  markMessageRecalled,
} from "@/p3/recall";
import { bytesToB64, b64ToBytes } from "@/relay/relayClient";
import { InMemoryLocalStore } from "@/store/localStore";
import type { MessageVM } from "@/types/viewModels";

beforeAll(() => {
  const g = globalThis as unknown as {
    btoa?: (s: string) => string;
    atob?: (s: string) => string;
  };
  if (!g.btoa) g.btoa = (s) => Buffer.from(s, "binary").toString("base64");
  if (!g.atob) g.atob = (s) => Buffer.from(s, "base64").toString("binary");
});

function outgoingMsg(clientMsgId: string, text: string): MessageVM {
  return {
    clientMsgId,
    convId: "d:5Bob",
    senderRef: "5Alice",
    isOutgoing: true,
    sentAt: Date.now(),
    content: { type: "text", text },
    mentions: [],
    starred: false,
    status: "acked",
    source: "offChainMls",
  };
}

describe("recall flow", () => {
  beforeAll(() => {
    keyVault.initForTest("recall-flow-seed");
  });

  it("markMessageRecalled blanks content and sets recalled status", async () => {
    const store = new InMemoryLocalStore();
    const msg = outgoingMsg("m1", "secret");
    await store.ensureConv("d:5Bob");
    await store.appendMessage(msg);

    const ok = await markMessageRecalled(store, "d:5Bob", "m1");
    expect(ok).toBe(true);

    const row = await store.getMessage("d:5Bob", "m1");
    expect(row?.status).toBe("recalled");
    expect(row?.content).toEqual({ type: "text", text: "" });
    expect(row?.starred).toBe(false);
  });

  it("markMessageRecalled is a no-op when the target is missing", async () => {
    const store = new InMemoryLocalStore();
    const ok = await markMessageRecalled(store, "d:5Bob", "missing");
    expect(ok).toBe(false);
  });

  it("applyRecallEnvelope ignores non-recall envelopes", async () => {
    const store = new InMemoryLocalStore();
    const ok = await applyRecallEnvelope(store, "d:5Bob", textEnvelope("x", "hi", {}));
    expect(ok).toBe(false);
  });

  it("delivers recall over the MLS relay frame path (sender → receiver)", async () => {
    const engine = new WebCryptoMlsEngine();
    const store = new InMemoryLocalStore();
    const convId = "d:5Bob";
    const targetId = "m-target";

    await store.ensureConv(convId);
    await store.appendMessage(outgoingMsg(targetId, "to recall"));

    const recall = recallEnvelope("rcl-1", targetId);
    const cipher = await engine.encrypt(convId, recall);
    const frame = { convId, ciphertextB64: bytesToB64(cipher) };

    const env = await engine.decrypt(frame.convId, b64ToBytes(frame.ciphertextB64));
    expect(env.type).toBe("recall");
    expect((env.body as { target: string }).target).toBe(targetId);

    const applied = await applyRecallEnvelope(store, convId, env);
    expect(applied).toBe(true);

    const row = await store.getMessage(convId, targetId);
    expect(row?.status).toBe("recalled");
    expect(row?.content).toEqual({ type: "text", text: "" });
  });

  it("sender local recall matches inbound handler (symmetric placeholder)", async () => {
    const store = new InMemoryLocalStore();
    const convId = "d:5Bob";
    const targetId = "m2";
    const msg = outgoingMsg(targetId, "symmetric");
    await store.ensureConv(convId);
    await store.appendMessage(msg);
    expect(canRecallMessage(msg)).toBe(true);

    await markMessageRecalled(store, convId, targetId);
    const senderView = await store.getMessage(convId, targetId);

    const store2 = new InMemoryLocalStore();
    await store2.ensureConv(convId);
    await store2.appendMessage({ ...msg, isOutgoing: false, senderRef: "5Alice" });
    const engine = new WebCryptoMlsEngine();
    const env = await engine.decrypt(
      convId,
      b64ToBytes(bytesToB64(await engine.encrypt(convId, recallEnvelope("rcl-2", targetId)))),
    );
    await applyRecallEnvelope(store2, convId, env);
    const receiverView = await store2.getMessage(convId, targetId);

    expect(senderView?.status).toBe("recalled");
    expect(receiverView?.status).toBe("recalled");
    expect(receiverView?.content).toEqual({ type: "text", text: "" });
  });
});
