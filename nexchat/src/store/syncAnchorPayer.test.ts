// EN: Frozen test vectors for the v2 burner payer (ADR §11.1, P3): the derivation must
// stay byte-identical forever — a silent change would orphan deposits already reserved
// under the old payer address. Vector chain: //Alice → vault_master (frozen in
// vaultMaster.test.ts) → payer seed → sr25519 address.
// CN: v2 burner payer 的冻结测试向量（ADR §11.1，P3）：派生必须永久字节一致——静默变更会
// 使旧 payer 地址下已 reserve 的押金失联。向量链：//Alice → vault_master（已在
// vaultMaster.test.ts 冻结）→ payer seed → sr25519 地址。

import { describe, expect, it } from "vitest";

import {
  deriveSyncPayerPair,
  payerTopUpAmount,
  PAYER_MIN_FREE,
  PAYER_TARGET_FREE,
} from "@/store/syncAnchorPayer";

// EN: = vault_master of //Alice (ALICE_MASTER_HEX in vaultMaster.test.ts).
// CN: 即 //Alice 的 vault_master（vaultMaster.test.ts 中的 ALICE_MASTER_HEX）。
const ALICE_MASTER_HEX = "086dbd77862df432e1122f8fbfe574de0cd4c8ceb8c7574d41c39002d4c12e2c";
const ALICE_PAYER_SS58 = "X4Z7CrkZonczRe4suPR6Qczj1QPhXGYZzwPqf68wxzV9tDJqc";

function fromHex(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

describe("deriveSyncPayerPair (frozen vector)", () => {
  it("derives the frozen payer address from Alice's vault_master", async () => {
    const pair = await deriveSyncPayerPair(fromHex(ALICE_MASTER_HEX));
    expect(pair.address).toBe(ALICE_PAYER_SS58);
  });

  it("is deterministic and differs from the main account", async () => {
    const a = await deriveSyncPayerPair(fromHex(ALICE_MASTER_HEX));
    const b = await deriveSyncPayerPair(fromHex(ALICE_MASTER_HEX));
    expect(a.address).toBe(b.address);
    // //Alice main address — the payer must never collide with it
    expect(a.address).not.toBe("X4Y9wZky3HPgyUGy5xH1RrwEVg3rTuzxYQ1GAKWscgAysZvxT");
  });

  it("different masters yield different payers", async () => {
    const other = await deriveSyncPayerPair(new Uint8Array(32).fill(7));
    expect(other.address).not.toBe(ALICE_PAYER_SS58);
  });
});

describe("payerTopUpAmount", () => {
  it("tops up to the target when below the minimum", () => {
    expect(payerTopUpAmount(0n)).toBe(PAYER_TARGET_FREE);
    expect(payerTopUpAmount(PAYER_MIN_FREE - 1n)).toBe(PAYER_TARGET_FREE - PAYER_MIN_FREE + 1n);
  });

  it("does nothing when funded", () => {
    expect(payerTopUpAmount(PAYER_MIN_FREE)).toBe(0n);
    expect(payerTopUpAmount(PAYER_TARGET_FREE * 10n)).toBe(0n);
  });
});
