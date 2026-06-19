import { describe, expect, it, beforeAll } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { formatChatDisplayName } from "@/chat/displayName";
import { nexDisplayAddress } from "@/wallet/address";
import type { MentionMember } from "@/p3/mentions";

let ALICE = "";
let BOB = "";
let roster: MentionMember[] = [];

beforeAll(async () => {
  await cryptoWaitReady();
  const kr = new Keyring({ type: "sr25519", ss58Format: 42 });
  ALICE = kr.addFromUri("//Alice").address;
  BOB = kr.addFromUri("//Bob").address;
  roster = [
    { ref: "Alice", address: ALICE, labels: ["Alice", ALICE] },
    { ref: "Bob", address: BOB, labels: ["Bob", BOB] },
  ];
});

describe("chat/displayName", () => {
  it("formats SS58 as nickname + last 4", () => {
    const suffix = nexDisplayAddress(ALICE).slice(-4);
    expect(formatChatDisplayName(ALICE, roster)).toBe(`Alice·${suffix}`);
  });

  it("formats nickname ref with roster address", () => {
    const suffix = nexDisplayAddress(BOB).slice(-4);
    expect(formatChatDisplayName("Bob", roster)).toBe(`Bob·${suffix}`);
  });

  it("uses fallback address for direct peer title", () => {
    const suffix = nexDisplayAddress(BOB).slice(-4);
    expect(
      formatChatDisplayName("新朋友", roster, { fallbackAddress: BOB }),
    ).toBe(`新朋友·${suffix}`);
  });

  it("formats self me with nickname", () => {
    const suffix = nexDisplayAddress(ALICE).slice(-4);
    expect(
      formatChatDisplayName("me", roster, {
        selfNickname: "小明",
        selfAddress: ALICE,
      }),
    ).toBe(`小明·${suffix}`);
  });
});
