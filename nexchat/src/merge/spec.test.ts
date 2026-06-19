// EN: Merge "constitution" tests — the CI must-pass gate (CHAT_FRONTEND_PLAN.md §3.1.4).
// Each case corresponds to a historical foot-gun. Any Merge change runs these first.
// CN: Merge「宪法」测试——前端 CI 必过门槛（CHAT_FRONTEND_PLAN.md §3.1.4）。
// 每条对应一个历史踩坑点；任何 Merge 改动都先跑它们。

import { describe, it, expect } from "vitest";
import {
  mergeConversations,
  appUnreadBadge,
  convKey,
  type OnChainRow,
  type LocalConv,
} from "./spec";

// 简单线性 block→time：每块 6s。
const blockToTime = (b: number) => b * 6000;

function directRow(p: Partial<OnChainRow> & { peer: string }): OnChainRow {
  return {
    kind: "direct",
    name: "",
    avatarCid: "",
    lastActive: 0,
    unread: 0,
    pinned: false,
    muted: false,
    archived: false,
    memberCount: 0,
    groupRole: 255,
    ...p,
  };
}

function groupRow(p: Partial<OnChainRow> & { groupId: number }): OnChainRow {
  return {
    kind: "group",
    name: `group-${p.groupId}`,
    avatarCid: "",
    lastActive: 0,
    unread: 0,
    pinned: false,
    muted: false,
    archived: false,
    memberCount: 3,
    groupRole: 2,
    ...p,
  };
}

describe("Merge constitution (T1–T10)", () => {
  it("T1: pure off-chain direct (no on-chain row) still appears", () => {
    const local: LocalConv[] = [
      { kind: "direct", peer: "alice", lastActive: 1000, unread: 2 },
    ];
    const out = mergeConversations([], local, blockToTime);
    expect(out).toHaveLength(1);
    expect(out[0].convId).toBe(convKey("direct", "alice"));
    expect(out[0].presence).toBe("offChainOnly");
    expect(out[0].recency).toBe(1000);
    expect(out[0].unread).toBe(2);
  });

  it("T2: System-only direct (no human chat) is kept as a notification card", () => {
    const onChain = [directRow({ peer: "platform", lastActive: 10, unread: 3 })];
    const out = mergeConversations(onChain, [], blockToTime);
    expect(out).toHaveLength(1);
    expect(out[0].presence).toBe("onChainOnly");
    // System unread surfaces on the card
    expect(out[0].unread).toBe(3);
  });

  it("T3: same peer with System + human MLS merges into ONE card", () => {
    const onChain = [directRow({ peer: "bob", lastActive: 5, unread: 1 })];
    const local: LocalConv[] = [
      { kind: "direct", peer: "bob", lastActive: 999_999, unread: 4 },
    ];
    const out = mergeConversations(onChain, local, blockToTime);
    expect(out).toHaveLength(1);
    expect(out[0].presence).toBe("both");
    // unread = local(4) + system(1) by default
    expect(out[0].unread).toBe(5);
  });

  it("T4: group last_active=0 on chain uses local recency, does NOT sink", () => {
    const onChain = [
      directRow({ peer: "carol", lastActive: 1 }), // -> time 6000
      groupRow({ groupId: 42, lastActive: 0 }),
    ];
    const local: LocalConv[] = [
      { kind: "group", groupId: 42, lastActive: 9_000_000, unread: 0 },
    ];
    const out = mergeConversations(onChain, local, blockToTime);
    // group has higher local recency -> sorts first
    expect(out[0].kind).toBe("group");
    expect(out[0].recency).toBe(9_000_000);
  });

  it("T5: direct muted resolves to dnd=true, adminMuted=false", () => {
    const onChain = [directRow({ peer: "dan", muted: true })];
    const out = mergeConversations(onChain, [], blockToTime);
    expect(out[0].dnd).toBe(true);
    expect(out[0].adminMuted).toBe(false);
  });

  it("T6: group muted resolves to adminMuted=true; dnd independent (local)", () => {
    const onChain = [groupRow({ groupId: 7, muted: true })];
    const local: LocalConv[] = [
      { kind: "group", groupId: 7, lastActive: 1, unread: 0, dndPref: false },
    ];
    const out = mergeConversations(onChain, local, blockToTime);
    expect(out[0].adminMuted).toBe(true);
    expect(out[0].dnd).toBe(false);
  });

  it("T7: app badge = sum of merged unread (incl. off-chain), not total_direct_unread", () => {
    const onChain = [
      directRow({ peer: "e", unread: 1 }),
      groupRow({ groupId: 1 }),
    ];
    const local: LocalConv[] = [
      { kind: "direct", peer: "e", lastActive: 1, unread: 10 },
      { kind: "group", groupId: 1, lastActive: 1, unread: 7 },
    ];
    const out = mergeConversations(onChain, local, blockToTime);
    // direct: 10 + 1(system) ; group: 7 -> total 18
    expect(appUnreadBadge(out)).toBe(18);
  });

  it("T8: pinned group sorts first, then by recency desc", () => {
    const onChain = [groupRow({ groupId: 9, memberCount: 3 })];
    const local: LocalConv[] = [
      { kind: "direct", peer: "x", lastActive: 5000, unread: 0 },
      { kind: "direct", peer: "y", lastActive: 9000, unread: 0 },
      { kind: "group", groupId: 9, lastActive: 1000, unread: 0, pinnedPref: true },
    ];
    const out = mergeConversations(onChain, local, blockToTime);
    expect(out[0].groupId).toBe(9); // pinned first despite lowest recency
    expect(out[0].pinned).toBe(true);
    expect(out[1].peer).toBe("y"); // then recency desc
    expect(out[2].peer).toBe("x");
  });

  it("T9: frozen group flag flows through for read-only UI", () => {
    const onChain = [groupRow({ groupId: 3, frozen: true })];
    const out = mergeConversations(onChain, [], blockToTime);
    expect(out[0].frozen).toBe(true);
  });

  it("T10: direct pinned/archived are chain-authoritative (OR local pin)", () => {
    const onChain = [directRow({ peer: "z", pinned: true, archived: true })];
    const local: LocalConv[] = [
      { kind: "direct", peer: "z", lastActive: 1, unread: 0, pinnedPref: false },
    ];
    const out = mergeConversations(onChain, local, blockToTime);
    expect(out[0].pinned).toBe(true); // chain pin wins
    expect(out[0].archived).toBe(true); // chain archive authoritative
  });

  it("T11: zero-member groups are omitted from the merged list", () => {
    const onChain = [
      groupRow({ groupId: 6, memberCount: 0 }),
      groupRow({ groupId: 7, memberCount: 2 }),
    ];
    const local: LocalConv[] = [
      { kind: "group", groupId: 6, lastActive: 1000, unread: 0 },
      { kind: "group", groupId: 99, lastActive: 2000, unread: 0 },
    ];
    const out = mergeConversations(onChain, local, blockToTime);
    expect(out.map((c) => c.groupId)).toEqual([7]);
  });
});
