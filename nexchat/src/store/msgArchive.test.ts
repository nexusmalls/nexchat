import { describe, it, expect, beforeEach } from "vitest";
import { keyVault } from "@/keyvault/keyvault";
import type { MessageVM } from "@/types/viewModels";
import { InMemoryLocalStore } from "@/store/localStore";
import {
  archiveToMessages,
  buildArchiveFromLocal,
  decryptArchiveBlob,
  encryptArchiveBlob,
  isArchivableMessage,
  mergeArchiveBlobs,
  tombstonesForRemovedMessages,
  type MessageArchiveBlob,
  type MsgArchiveEntry,
} from "@/store/msgArchive";

function textMsg(convId: string, id: string, text: string, sentAt: number): MessageVM {
  return {
    clientMsgId: id,
    convId,
    senderRef: "me",
    isOutgoing: true,
    sentAt,
    content: { type: "text", text },
    mentions: [],
    starred: false,
    status: "acked",
    source: "offChainMls",
  };
}

describe("msgArchive", () => {
  beforeEach(() => {
    keyVault.initForTest("5AliceTestAccount");
  });

  it("isArchivableMessage rejects ephemeral rows", () => {
    const eph = textMsg("d:bob", "e1", "bye", 1);
    eph.ephemeralTtlMs = 5000;
    expect(isArchivableMessage(eph)).toBe(false);
    expect(isArchivableMessage(textMsg("d:bob", "m1", "hi", 1))).toBe(true);
  });

  it("isArchivableMessage allows recalled rows (placeholder must sync across devices)", () => {
    const recalled = textMsg("d:bob", "r1", "", 1);
    recalled.status = "recalled";
    expect(isArchivableMessage(recalled)).toBe(true);
  });

  it("mergeArchiveBlobs lets a recalled entry dominate regardless of updated_at", () => {
    const base = (
      text: string,
      updated_at: number,
      status: MessageVM["status"],
    ): MsgArchiveEntry => ({
      client_msg_id: "m1",
      conv_id: "d:bob",
      sender_ref: "me",
      is_outgoing: true,
      sent_at: 100,
      content: { type: "text", text },
      mentions: [],
      starred: false,
      status,
      source: "offChainMls",
      updated_at,
    });
    // recalled side has the OLDER updated_at, yet must still win (recall is terminal).
    const recalledOld: MessageArchiveBlob = {
      v: 1,
      updated_at: 100,
      device_id: "a",
      conversations: [
        { conv_id: "d:bob", messages: [base("", 100, "recalled")], updated_at: 100 },
      ],
    };
    const originalNew: MessageArchiveBlob = {
      v: 1,
      updated_at: 999,
      device_id: "b",
      conversations: [
        { conv_id: "d:bob", messages: [base("secret", 999, "acked")], updated_at: 999 },
      ],
    };
    const m1 = mergeArchiveBlobs(recalledOld, originalNew);
    expect(m1.conversations[0]!.messages[0]!.status).toBe("recalled");
    expect(m1.conversations[0]!.messages[0]!.content).toEqual({ type: "text", text: "" });
    // order-independent
    const m2 = mergeArchiveBlobs(originalNew, recalledOld);
    expect(m2.conversations[0]!.messages[0]!.status).toBe("recalled");
  });

  it("mergeArchiveBlobs lets a tombstone dominate regardless of updated_at", () => {
    const live = (
      text: string,
      updated_at: number,
    ): MsgArchiveEntry => ({
      client_msg_id: "m1",
      conv_id: "d:bob",
      sender_ref: "me",
      is_outgoing: true,
      sent_at: 100,
      content: { type: "text", text },
      mentions: [],
      starred: false,
      status: "acked",
      source: "offChainMls",
      updated_at,
    });
    const tombOld: MessageArchiveBlob = {
      v: 1,
      updated_at: 100,
      device_id: "a",
      conversations: [
        {
          conv_id: "d:bob",
          messages: [{ ...live("gone", 100), tombstone: true }],
          updated_at: 100,
        },
      ],
    };
    const liveNew: MessageArchiveBlob = {
      v: 1,
      updated_at: 999,
      device_id: "b",
      conversations: [
        { conv_id: "d:bob", messages: [live("still here", 999)], updated_at: 999 },
      ],
    };
    const m1 = mergeArchiveBlobs(tombOld, liveNew);
    expect(m1.conversations[0]!.messages[0]!.tombstone).toBe(true);
    const m2 = mergeArchiveBlobs(liveNew, tombOld);
    expect(m2.conversations[0]!.messages[0]!.tombstone).toBe(true);
  });

  it("mergeArchiveBlobs lets a tombstone beat a newer recalled placeholder", () => {
    const recalled: MsgArchiveEntry = {
      client_msg_id: "m1",
      conv_id: "d:bob",
      sender_ref: "me",
      is_outgoing: true,
      sent_at: 100,
      content: { type: "text", text: "" },
      mentions: [],
      starred: false,
      status: "recalled",
      source: "offChainMls",
      updated_at: 999,
    };
    const tomb: MsgArchiveEntry = {
      ...recalled,
      tombstone: true,
      updated_at: 100,
    };
    const recalledBlob: MessageArchiveBlob = {
      v: 1,
      updated_at: 999,
      device_id: "b",
      conversations: [{ conv_id: "d:bob", messages: [recalled], updated_at: 999 }],
    };
    const tombBlob: MessageArchiveBlob = {
      v: 1,
      updated_at: 100,
      device_id: "a",
      conversations: [{ conv_id: "d:bob", messages: [tomb], updated_at: 100 }],
    };
    const m = mergeArchiveBlobs(recalledBlob, tombBlob);
    expect(m.conversations[0]!.messages[0]!.tombstone).toBe(true);
  });

  it("mergeArchiveBlobs keeps newer message row", () => {
    const entry = (text: string, updated_at: number): MsgArchiveEntry => ({
      client_msg_id: "m1",
      conv_id: "d:bob",
      sender_ref: "me",
      is_outgoing: true,
      sent_at: 100,
      content: { type: "text", text },
      mentions: [],
      starred: false,
      status: "acked",
      source: "offChainMls",
      updated_at,
    });
    const a: MessageArchiveBlob = {
      v: 1,
      updated_at: 100,
      device_id: "a",
      conversations: [{ conv_id: "d:bob", messages: [entry("old", 100)], updated_at: 100 }],
    };
    const b: MessageArchiveBlob = {
      v: 1,
      updated_at: 200,
      device_id: "b",
      conversations: [{ conv_id: "d:bob", messages: [entry("new", 200)], updated_at: 200 }],
    };
    const m = mergeArchiveBlobs(a, b);
    expect(m.conversations[0]!.messages[0]!.content).toEqual({ type: "text", text: "new" });
  });

  it("buildArchiveFromLocal + encrypt/decrypt round-trip", async () => {
    const store = new InMemoryLocalStore();
    await store.ensureConv("d:5Bob");
    await store.appendMessage(textMsg("d:5Bob", "m1", "hello", 1000));
    const blob = await buildArchiveFromLocal(store, 500, "dev-1", 42);
    const packed = await encryptArchiveBlob(blob);
    const back = await decryptArchiveBlob(packed);
    const msgs = archiveToMessages(back);
    expect(msgs).toHaveLength(1);
    expect(msgs[0]!.content).toEqual({ type: "text", text: "hello" });
  });

  it("tombstonesForRemovedMessages marks deleted rows", async () => {
    const store = new InMemoryLocalStore();
    await store.ensureConv("d:5Bob");
    await store.appendMessage(textMsg("d:5Bob", "m1", "keep", 1));
    const local = await buildArchiveFromLocal(store, 500, "d", 10);
    const last: MessageArchiveBlob = {
      v: 1,
      updated_at: 5,
      device_id: "x",
      conversations: [
        {
          conv_id: "d:5Bob",
          updated_at: 5,
          messages: [
            {
              client_msg_id: "m1",
              conv_id: "d:5Bob",
              sender_ref: "me",
              is_outgoing: true,
              sent_at: 1,
              content: { type: "text", text: "keep" },
              mentions: [],
              starred: false,
              status: "acked",
              source: "offChainMls",
              updated_at: 1,
            },
            {
              client_msg_id: "m2",
              conv_id: "d:5Bob",
              sender_ref: "me",
              is_outgoing: true,
              sent_at: 2,
              content: { type: "text", text: "gone" },
              mentions: [],
              starred: false,
              status: "acked",
              source: "offChainMls",
              updated_at: 2,
            },
          ],
        },
      ],
    };
    const tombs = tombstonesForRemovedMessages(last, local, 99);
    expect(tombs).toHaveLength(1);
    expect(tombs[0]!.client_msg_id).toBe("m2");
    expect(tombs[0]!.tombstone).toBe(true);
  });
});
