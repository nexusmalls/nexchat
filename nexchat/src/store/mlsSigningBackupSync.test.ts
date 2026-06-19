import { describe, expect, it } from "vitest";
import {
  handoffTransferAfterPinRestore,
  nextSigningBackupSeq,
} from "@/store/mlsSigningBackupSync";

describe("nextSigningBackupSeq", () => {
  it("is strictly greater than the previous seq", () => {
    expect(nextSigningBackupSeq(null)).toBeGreaterThan(0);
    expect(nextSigningBackupSeq({ cid: "x", updated_at: 1000 })).toBeGreaterThan(1000);
    expect(nextSigningBackupSeq({ cid: "x", updated_at: 1000 })).toBeGreaterThanOrEqual(1001);
  });
});

describe("handoffTransferAfterPinRestore", () => {
  const receipt = (to: string) => ({ v: 1 as const, from: "old", to, seq: 3, ts: 1 });

  it("returns null when there is no prior receipt (§5.1 bootstrap)", () => {
    expect(handoffTransferAfterPinRestore({ latestReceipt: null, selfDeviceId: "me" })).toBeNull();
  });

  it("returns null when this device already holds authority", () => {
    expect(
      handoffTransferAfterPinRestore({ latestReceipt: receipt("me"), selfDeviceId: "me" }),
    ).toBeNull();
  });

  it("transfers from the current holder to this device when another device holds it", () => {
    expect(
      handoffTransferAfterPinRestore({ latestReceipt: receipt("other"), selfDeviceId: "me" }),
    ).toEqual({ from: "other", to: "me" });
  });
});
