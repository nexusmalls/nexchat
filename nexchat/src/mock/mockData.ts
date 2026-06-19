// EN: Fixtures for VITE_USE_MOCK=true — exercises every Merge branch so the UI
// can be developed/demoed without a running node.
// CN: VITE_USE_MOCK=true 的夹具——覆盖每条 Merge 分支，无需节点即可开发/演示 UI。

import type { OnChainRow, LocalConv } from "@/merge/spec";
import type { MessageVM } from "@/types/viewModels";

const now = Date.now();
const min = 60_000;

export function mockOnChainRows(): OnChainRow[] {
  return [
    // System-only direct (platform notification) — T2
    {
      kind: "direct",
      peer: "5Platform...Notif",
      directId: "0xsys",
      name: "",
      avatarCid: "",
      lastActive: 100,
      unread: 1,
      pinned: false,
      muted: false,
      archived: false,
      memberCount: 0,
      groupRole: 255,
    },
    // Direct that also has off-chain human chat — T3 (merges with local 'bob')
    {
      kind: "direct",
      peer: "5Bob...xyz",
      directId: "0xbob",
      name: "",
      avatarCid: "",
      lastActive: 90,
      unread: 0,
      pinned: true, // chain-authoritative pin
      muted: false,
      archived: false,
      memberCount: 0,
      groupRole: 255,
    },
    // Group where I'm admin-muted — T6
    {
      kind: "group",
      groupId: 42,
      name: "NEX 做市群",
      avatarCid: "",
      lastActive: 0,
      unread: 0,
      pinned: false,
      muted: true, // admin mute => cannot send
      archived: false,
      memberCount: 5,
      groupRole: 2,
      frozen: false,
    },
    // Frozen group — T9
    {
      kind: "group",
      groupId: 7,
      name: "已冻结群",
      avatarCid: "",
      lastActive: 0,
      unread: 0,
      pinned: false,
      muted: false,
      archived: false,
      memberCount: 4,
      groupRole: 1,
      frozen: true,
    },
  ];
}

export function mockLocalConvs(): LocalConv[] {
  return [
    // Pure off-chain direct (no on-chain row) — T1
    {
      kind: "direct",
      peer: "5Alice...abc",
      lastActive: now - 2 * min,
      unread: 2,
      title: "Alice",
      lastMessagePreview: "在吗？看下那个订单",
    },
    // Off-chain human chat for bob (merges with on-chain System row) — T3
    {
      kind: "direct",
      peer: "5Bob...xyz",
      lastActive: now - 30 * min,
      unread: 4,
      title: "Bob",
      lastMessagePreview: "好的，我稍后转账",
      dndPref: true,
    },
    // Local state for the admin-muted group
    {
      kind: "group",
      groupId: 42,
      lastActive: now - 5 * min,
      unread: 12,
      lastMessagePreview: "今天的报价是…",
    },
    // Pinned group (client-side pin only) — T8
    {
      kind: "group",
      groupId: 99,
      lastActive: now - 90 * min,
      unread: 0,
      pinnedPref: true,
      title: "我的收藏群",
      lastMessagePreview: "（置顶）公告",
    },
  ];
}

export function mockMessages(convId: string): MessageVM[] {
  const base: Record<string, MessageVM[]> = {
    "d:5Alice...abc": [
      msg(convId, "m1", "Alice", false, now - 3 * min, { type: "text", text: "在吗？" }),
      msg(convId, "m2", "Alice", false, now - 2 * min, {
        type: "text",
        text: "看下那个订单",
      }),
    ],
    "d:5Bob...xyz": [
      msg(convId, "m3", "Bob", false, now - 40 * min, {
        type: "text",
        text: "收到货了吗",
      }),
      msg(convId, "m4", "me", true, now - 35 * min, { type: "text", text: "收到了" }),
      msg(convId, "m5", "Bob", false, now - 30 * min, {
        type: "text",
        text: "好的，我稍后转账",
      }),
      msg(convId, "sys1", "System", false, now - 31 * min, {
        type: "system",
        kind: "订单 #1024 已创建",
      }),
    ],
    "g:42": [
      msg(convId, "g1", "Carol", false, now - 6 * min, {
        type: "text",
        text: "今天的报价是…",
      }),
    ],
    "d:5Platform...Notif": [
      msg(convId, "n1", "System", false, now - 600 * min, {
        type: "system",
        kind: "争议 #88 进入仲裁",
      }),
    ],
  };
  return base[convId] ?? [];
}

function msg(
  convId: string,
  id: string,
  senderRef: string,
  isOutgoing: boolean,
  sentAt: number,
  content: MessageVM["content"],
): MessageVM {
  return {
    clientMsgId: id,
    convId,
    senderRef,
    isOutgoing,
    sentAt,
    content,
    mentions: [],
    starred: false,
    status: isOutgoing ? "acked" : "acked",
    source: content.type === "system" ? "onChainSystem" : "offChainMls",
  };
}
