// EN: Eventual-consistency gap refill (hybrid design §4.5): a newly-grafted device re-pulls the archive
// on a bounded delayed schedule so messages the peer sent BEFORE the graft — archived by an online
// sibling shortly after — eventually merge into local timelines. CN: 最终一致性空窗补齐（混合设计 §4.5）：
// 新嫁接设备按有界延迟重拉 archive，使对端在嫁接**前**所发、在线兄弟稍后归档的消息最终合并进本地时间线。

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  ipfs: new Map<string, Uint8Array>(),
  ptr: null as { cid: string; updated_at: number } | null,
}));

vi.mock("@/config", async (importOriginal) => {
  const mod = await importOriginal<typeof import("@/config")>();
  return { config: { ...mod.config, msgArchiveEnabled: true, ipfsEnabled: true } };
});

vi.mock("@/ipfs/ipfsClient", () => ({
  ipfsClient: {
    add: async (data: Uint8Array) => {
      const cid = `cid-${h.ipfs.size + 1}`;
      h.ipfs.set(cid, data);
      return cid;
    },
    cat: async (cid: string) => {
      const d = h.ipfs.get(cid);
      if (!d) throw new Error(`no cid ${cid}`);
      return d;
    },
  },
}));

vi.mock("@/relay/msgArchivePointer", () => ({
  fetchMsgArchivePointer: async () => h.ptr,
  publishMsgArchivePointer: async (_a: string, p: { cid: string; updated_at: number }) => {
    h.ptr = p;
  },
  readLocalMsgArchivePointer: () => null,
}));

import { keyVault } from "@/keyvault/keyvault";
import { InMemoryLocalStore } from "@/store/localStore";
import { buildArchiveFromLocal, encryptArchiveBlob } from "@/store/msgArchive";
import { MsgArchiveSync } from "@/store/msgArchiveSync";
import type { MessageVM } from "@/types/viewModels";

const ACCT = "5AliceTestAccount";

function textMsg(convId: string, id: string, text: string, sentAt: number): MessageVM {
  return {
    clientMsgId: id,
    convId,
    senderRef: "bob",
    isOutgoing: false,
    sentAt,
    content: { type: "text", text },
    mentions: [],
    starred: false,
    status: "acked",
    source: "offChainMls",
  };
}

/// EN: Simulate an online sibling pushing an archive snapshot containing `msgs`. CN: 模拟在线兄弟推送
/// 含 `msgs` 的归档快照。
async function publishRemote(msgs: MessageVM[], updatedAt: number): Promise<void> {
  const tmp = new InMemoryLocalStore();
  for (const m of msgs) {
    await tmp.ensureConv(m.convId);
    await tmp.appendMessage(m);
  }
  const blob = { ...(await buildArchiveFromLocal(tmp, 1000, "sibling")), updated_at: updatedAt };
  const packed = await encryptArchiveBlob(blob);
  const cid = `remote-${updatedAt}`;
  h.ipfs.set(cid, packed);
  h.ptr = { cid, updated_at: updatedAt };
}

async function ids(store: InMemoryLocalStore, conv: string): Promise<string[]> {
  return (await store.listMessages(conv)).map((m) => m.clientMsgId).sort();
}

describe("MsgArchiveSync gap refill (§4.5)", () => {
  beforeEach(() => {
    h.ipfs.clear();
    h.ptr = null;
    keyVault.initForTest(ACCT);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("restore() merges a newer remote snapshot (the gap-fill primitive)", async () => {
    const store = new InMemoryLocalStore();
    const sync = new MsgArchiveSync(store, false);

    await publishRemote([textMsg("d:bob", "m1", "hi", 100)], 1_000);
    expect(await sync.restore(ACCT)).toBe(true);
    expect(await ids(store, "d:bob")).toEqual(["m1"]);

    // EN: sibling later archives a message the peer sent in the middle window. CN: 兄弟稍后归档对端在
    // 空窗内所发消息。
    await publishRemote(
      [textMsg("d:bob", "m1", "hi", 100), textMsg("d:bob", "m2", "gap", 200)],
      2_000,
    );
    expect(await sync.restore(ACCT)).toBe(true);
    expect(await ids(store, "d:bob")).toEqual(["m1", "m2"]);
  });

  it("scheduleGapRefill re-pulls on a delay and fires onApplied when the gap closes", async () => {
    const store = new InMemoryLocalStore();
    const sync = new MsgArchiveSync(store, false);

    // EN: device joins with only m1 visible (its unlock-time snapshot). CN: 设备加入时仅可见 m1（解锁
    // 时快照）。
    await publishRemote([textMsg("d:bob", "m1", "hi", 100)], 1_000);
    expect(await sync.restore(ACCT)).toBe(true);
    expect(await ids(store, "d:bob")).toEqual(["m1"]);

    // EN: sibling archives the gap message AFTER the join. CN: 兄弟在加入**后**归档空窗消息。
    await publishRemote(
      [textMsg("d:bob", "m1", "hi", 100), textMsg("d:bob", "m2", "gap", 200)],
      2_000,
    );

    let applied = 0;
    sync.scheduleGapRefill(
      ACCT,
      () => {
        applied += 1;
      },
      [5],
    );

    // EN: nothing yet before the scheduled delay. CN: 调度延迟前不应触发。
    expect(await ids(store, "d:bob")).toEqual(["m1"]);

    await new Promise((r) => setTimeout(r, 40));
    expect(await ids(store, "d:bob")).toEqual(["m1", "m2"]);
    expect(applied).toBeGreaterThan(0);
  });

  it("scheduleGapRefill coalesces — re-scheduling resets the pending sequence", async () => {
    const store = new InMemoryLocalStore();
    const sync = new MsgArchiveSync(store, false);
    await publishRemote([textMsg("d:bob", "m1", "hi", 100)], 1_000);

    let applied = 0;
    const bump = () => {
      applied += 1;
    };
    sync.scheduleGapRefill(ACCT, bump, [5]);
    // EN: re-schedule before the first refill fires → only the second sequence remains. CN: 首次重拉前
    // 重新调度 → 仅保留第二个序列。
    sync.scheduleGapRefill(ACCT, bump, [5]);

    await new Promise((r) => setTimeout(r, 40));
    // EN: exactly one refill (the coalesced sequence), not two. CN: 恰好一次重拉（合并后的序列），而非两次。
    expect(applied).toBe(1);
  });
});
