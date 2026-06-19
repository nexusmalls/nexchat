// EN: InboxManager epoch-bump tests (§6.5 ③ — the action that closes the spent-replay
// window after a chain-anchor recovery).
// CN: InboxManager epoch bump 单测（§6.5 ③——链锚恢复后关闭 spent 重放窗口的动作）。

import { beforeEach, describe, expect, it, vi } from "vitest";

const sent = vi.hoisted(() => [] as Array<Record<string, unknown>>);

const relayOneShotSend = vi.hoisted(() =>
  vi.fn(async (_account: string, msg: Record<string, unknown>) => {
    sent.push(msg);
  }),
);

vi.mock("@/config", () => ({
  config: { relayWs: "ws://test-relay", deliveryModulusBits: 2048 },
}));

vi.mock("@/relay/relayOneShot", () => ({
  relayOneShotSend,
}));

import { RelayInboxStaleEpochError } from "@/relay/relayErrors";
import { InboxManager } from "@/delivery/inboxManager";

function memMeta() {
  const map = new Map<string, unknown>();
  return {
    map,
    getMeta: async <T,>(key: string) => (map.get(key) as T) ?? null,
    setMeta: async <T,>(key: string, value: T) => {
      map.set(key, value);
    },
  };
}

const ACCOUNT = "5TestAccountAddress";

describe("InboxManager.bumpEpoch (§6.5 ③)", () => {
  beforeEach(() => {
    sent.length = 0;
    relayOneShotSend.mockReset();
    relayOneShotSend.mockImplementation(async (_account: string, msg: Record<string, unknown>) => {
      sent.push(msg);
    });
  });

  it("increments the epoch, persists it, and re-registers with the new epoch", async () => {
    const meta = memMeta();
    const mgr = new InboxManager(meta);
    const rec = await mgr.ensure(ACCOUNT);
    expect(rec.epoch).toBe(0);

    const epoch = await mgr.bumpEpoch(ACCOUNT);
    expect(epoch).toBe(1);

    // Persisted: a fresh manager (new device session) sees the bumped epoch.
    // 已持久化：新会话的 manager 读到 bump 后的 epoch。
    const mgr2 = new InboxManager(meta);
    const rec2 = await mgr2.ensure(ACCOUNT);
    expect(rec2.epoch).toBe(1);
    expect(rec2.inboxId).toBe(rec.inboxId); // key pair unchanged / 密钥对不变

    // Relay re-registration carried the new epoch. / relay 重注册带新 epoch。
    const reg = sent.filter((m) => m.type === "inbox_register");
    expect(reg.length).toBe(1);
    expect(reg[0]!.epoch).toBe(1);
    expect(reg[0]!.inbox_id).toBe(rec.inboxId);
  });

  it("creates the record first when bumping on a fresh device (mnemonic-only restore)", async () => {
    const meta = memMeta();
    const mgr = new InboxManager(meta);
    // No ensure() beforehand — disaster-recovery path may bump before any send.
    // 事先未 ensure()——灾后恢复路径可能在任何发送前先 bump。
    const epoch = await mgr.bumpEpoch(ACCOUNT);
    expect(epoch).toBe(1);
    expect(mgr.get()?.epoch).toBe(1);
  });

  it("bumps are cumulative across calls", async () => {
    const meta = memMeta();
    const mgr = new InboxManager(meta);
    await mgr.ensure(ACCOUNT);
    await mgr.bumpEpoch(ACCOUNT);
    await mgr.bumpEpoch(ACCOUNT);
    expect(mgr.get()?.epoch).toBe(2);
  });

  it("adopts relay epoch and retries once on inbox_reject stale_epoch", async () => {
    relayOneShotSend.mockReset();
    sent.length = 0;
    relayOneShotSend
      .mockRejectedValueOnce(new RelayInboxStaleEpochError(7))
      .mockImplementation(async (_account: string, msg: Record<string, unknown>) => {
        sent.push(msg);
      });
    const meta = memMeta();
    const mgr = new InboxManager(meta);
    await mgr.ensure(ACCOUNT);
    await mgr.registerRelay(ACCOUNT);
    expect(mgr.get()?.epoch).toBe(7);
    expect(relayOneShotSend).toHaveBeenCalledTimes(2);
    expect(sent[0]?.epoch).toBe(7);
  });
});
