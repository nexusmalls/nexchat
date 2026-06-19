// EN: Proves the encrypted local store persists the message timeline + conv prefs across a
// simulated refresh (a fresh store instance over the same IndexedDB), and that rows are
// ciphertext at rest. CN: 验证加密本地库跨“模拟刷新”（同一 IndexedDB 上的新实例）保留消息
// 时间线 + 会话偏好，且行在静态时为密文。

import "fake-indexeddb/auto";
import { describe, it, expect } from "vitest";
import { EncryptedIdbLocalStore } from "@/store/encryptedLocalStore";
import { keyVault } from "@/keyvault/keyvault";
import type { MessageVM } from "@/types/viewModels";

keyVault.initForTest("test-seed");

function msg(convId: string, id: string, text: string, sentAt: number): MessageVM {
  return {
    convId,
    clientMsgId: id,
    senderRef: "self",
    isOutgoing: true,
    sentAt,
    status: "sent",
    content: { type: "text", text },
    mentions: [],
    starred: false,
    source: "offChainMls",
  };
}

describe("EncryptedIdbLocalStore", () => {
  it("persists messages + conv state across a simulated refresh", async () => {
    const ns = `acct-${Date.now()}`;
    const convId = "g:7";

    const a = new EncryptedIdbLocalStore();
    await a.open(ns);
    await a.ensureConv(convId);
    await a.appendMessage(msg(convId, "m1", "hello", 1000));
    await a.appendMessage(msg(convId, "m2", "world", 2000));
    await a.bumpUnread(convId);
    await a.setPref(convId, { pinnedPref: true });

    // EN: a brand-new instance == a page refresh. CN: 全新实例 == 页面刷新。
    const b = new EncryptedIdbLocalStore();
    await b.open(ns);

    const msgs = await b.listMessages(convId);
    expect(msgs.map((m) => (m.content as { text: string }).text)).toEqual(["hello", "world"]);

    const convs = await b.listLocalConvs();
    expect(convs).toHaveLength(1);
    expect(convs[0].pinnedPref).toBe(true);
    expect(convs[0].unread).toBe(1);
    expect(convs[0].lastMessagePreview).toBe("world");
    expect(convs[0].lastActive).toBe(2000);

    const one = await b.getMessage(convId, "m1");
    expect((one!.content as { text: string }).text).toBe("hello");
  });

  it("stores rows as ciphertext (no plaintext leaks to IndexedDB)", async () => {
    const ns = `acct-cipher-${Date.now()}`;
    const store = new EncryptedIdbLocalStore();
    await store.open(ns);
    await store.ensureConv("g:9");
    await store.appendMessage(msg("g:9", "secret-id", "TOP-SECRET-PLAINTEXT", 5000));

    const raw: unknown[] = await new Promise((resolve, reject) => {
      const open = indexedDB.open(`nexchat-local-${ns}`, 2);
      open.onsuccess = () => {
        const tx = open.result.transaction("messages", "readonly");
        const all = tx.objectStore("messages").getAll();
        all.onsuccess = () => resolve(all.result as unknown[]);
        all.onerror = () => reject(all.error);
      };
      open.onerror = () => reject(open.error);
    });

    expect(raw).toHaveLength(1);
    const blob = JSON.stringify(raw[0]);
    expect(blob).not.toContain("TOP-SECRET-PLAINTEXT");
    expect(raw[0]).toHaveProperty("iv");
    expect(raw[0]).toHaveProperty("ct");
  });

  // EN: §5.0 one-time migration — rows sealed under the legacy (address-derived) root are
  // re-sealed under the vault_master root on first open. CN: §5.0 一次性迁移——旧（地址派生）
  // 根封装的行在首次打开时重封到 vault_master 根。
  it("re-seals legacy rows under the vault_master root (one-time §5.0 migration)", async () => {
    const ns = `acct-migrate-${Date.now()}`;
    const convId = "g:1";

    // 1) Legacy world: root = SHA-256(seed string), like the pre-§5.0 address root.
    keyVault.initForTest(ns);
    const legacy = new EncryptedIdbLocalStore();
    await legacy.open(ns);
    await legacy.ensureConv(convId);
    await legacy.appendMessage(msg(convId, "m1", "migrate-me", 1000));
    await legacy.setMeta("inbox", { id: "abc" });

    // 2) Unlock with the new vault_master root; the old seed is passed as the LEGACY base.
    const master = crypto.getRandomValues(new Uint8Array(32));
    keyVault.init(master, { legacySeed: ns });
    const fresh = new EncryptedIdbLocalStore();
    await fresh.open(ns);
    const msgs = await fresh.listMessages(convId);
    expect(msgs.map((m) => (m.content as { text: string }).text)).toEqual(["migrate-me"]);
    expect(await fresh.getMeta("inbox")).toEqual({ id: "abc" });

    // 3) Proof of re-seal: a reader holding ONLY the legacy root can no longer decrypt.
    keyVault.initForTest(ns);
    const legacyReader = new EncryptedIdbLocalStore();
    await legacyReader.open(ns);
    expect(await legacyReader.listMessages(convId)).toHaveLength(0);
    expect(await legacyReader.getMeta("inbox")).toBeNull();

    keyVault.initForTest("test-seed");
  });
});
