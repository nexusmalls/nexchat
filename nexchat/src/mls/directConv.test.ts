import { describe, expect, it } from "vitest";
import { peerFromMlsKey, resolveDirectInboundConv } from "@/mls/directConv";
import type { RelayFrame } from "@/relay/relayClient";
import { canonicalAddress } from "@/wallet/address";

// EN: canonicalize fixtures so they match the SS58 prefix the routing code normalizes to
// (prefix-agnostic; survives prefix changes). CN: 夹具走 canonicalAddress，与路由代码归一化
// 的 SS58 前缀一致（前缀无关，前缀变更也不破）。
const ALICE = canonicalAddress("5FWAPJTdPGDtekamVhK4KL3Ee2gkVu4jC8JVRRanJeN1FvMo");
const BOB = canonicalAddress("5GseotPUjm5GCYw2wr3H4Ce7rfeG882rFLCUEWZVKzAY3BN8");
const MLS_KEY = `d:${[ALICE, BOB].sort().join(":")}`;

describe("directConv inbound routing", () => {
  it("peerFromMlsKey returns counterparty", () => {
    expect(peerFromMlsKey(MLS_KEY, BOB)).toBe(ALICE);
    expect(peerFromMlsKey(MLS_KEY, ALICE)).toBe(BOB);
  });

  it("resolveDirectInboundConv remaps via delivery.mlsKey when senderRef empty", () => {
    const frame: RelayFrame = {
      convId: `d:${BOB}`,
      senderRef: "",
      ciphertextB64: "AA==",
      delivery: {
        inboxId: "0x1",
        ipkN: "n",
        ipkE: "AQAB",
        epoch: 0,
        ct: "c",
        t: "t",
        s: "s",
        p: "p",
        mlsKey: MLS_KEY,
      },
    };
    const out = resolveDirectInboundConv(frame, BOB, "peer");
    expect(out.convId).toBe(`d:${ALICE}`);
    expect(out.senderRef).toBe(ALICE);
  });

  it("resolveDirectInboundConv ignores nickname senderRef and uses mlsKey", () => {
    const frame: RelayFrame = {
      convId: `d:${BOB}`,
      senderRef: "wer",
      ciphertextB64: "AA==",
      delivery: { inboxId: "0x1", ipkN: "n", ipkE: "AQAB", epoch: 0, ct: "c", t: "t", s: "s", p: "p", mlsKey: MLS_KEY },
    };
    const out = resolveDirectInboundConv(frame, BOB, "wer");
    expect(out.convId).toBe(`d:${ALICE}`);
    expect(out.senderRef).toBe(ALICE);
  });
});
