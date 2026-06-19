// EN: Tests for Track A sending-authority primitives (design §5): device-directory key derivation,
// HandoffReceipt sign/verify, and the pure authority arithmetic (latest-wins, send guard).
// CN: 路线 A 发送权原语（设计 §5）单测：设备目录钥派生、HandoffReceipt 签/验、纯权威裁决（最新胜出、
// 发送守卫）。

import { describe, expect, it } from "vitest";

import {
  buildNextReceipt,
  canSend,
  compareReceipts,
  deriveDeviceDirectoryKey,
  pickLatestReceipt,
  resolveAuthoritativeDevice,
  signHandoffReceipt,
  verifyHandoffReceipt,
  type HandoffReceipt,
  type SignedHandoffReceipt,
} from "@/mls/sendingAuthority";

const master = (byte: number) => new Uint8Array(32).fill(byte);
const receipt = (over: Partial<HandoffReceipt> = {}): HandoffReceipt => ({
  v: 1,
  from: "devA",
  to: "devB",
  seq: 1,
  ts: 1000,
  ...over,
});

describe("device-directory key + HandoffReceipt sign/verify (§5.2)", () => {
  it("is deterministic from vault_master and verifies a signed receipt", async () => {
    const k1 = await deriveDeviceDirectoryKey(master(7));
    const k2 = await deriveDeviceDirectoryKey(master(7));
    expect(toHex(k1.publicKey)).toEqual(toHex(k2.publicKey));

    const r = receipt({ seq: 3, ts: 5000 });
    const sig = await signHandoffReceipt(k1, r);
    expect(await verifyHandoffReceipt(k1.publicKey, { receipt: r, sig })).toBe(true);
  });

  it("rejects a tampered receipt and a wrong directory key", async () => {
    const dir = await deriveDeviceDirectoryKey(master(9));
    const other = await deriveDeviceDirectoryKey(master(10));
    const r = receipt({ seq: 4 });
    const sig = await signHandoffReceipt(dir, r);

    // tampered field → signature no longer matches.
    expect(
      await verifyHandoffReceipt(dir.publicKey, { receipt: { ...r, to: "devEvil" }, sig }),
    ).toBe(false);
    // different account directory key → reject.
    expect(await verifyHandoffReceipt(other.publicKey, { receipt: r, sig })).toBe(false);
  });

  it("derives distinct keys for distinct vault_master roots", async () => {
    const a = await deriveDeviceDirectoryKey(master(1));
    const b = await deriveDeviceDirectoryKey(master(2));
    expect(toHex(a.publicKey)).not.toEqual(toHex(b.publicKey));
  });
});

describe("authority arithmetic (§5.4, pure)", () => {
  it("orders receipts by seq then ts", () => {
    expect(compareReceipts(receipt({ seq: 2 }), receipt({ seq: 1 }))).toBeGreaterThan(0);
    expect(compareReceipts(receipt({ seq: 1, ts: 10 }), receipt({ seq: 1, ts: 20 }))).toBeLessThan(0);
    expect(compareReceipts(receipt({ seq: 1, ts: 5 }), receipt({ seq: 1, ts: 5 }))).toBe(0);
  });

  it("picks the latest receipt (greatest seq) regardless of input order", () => {
    const set: SignedHandoffReceipt[] = [
      { receipt: receipt({ seq: 1, to: "d1" }), sig: "0x00" },
      { receipt: receipt({ seq: 3, to: "d3" }), sig: "0x00" },
      { receipt: receipt({ seq: 2, to: "d2" }), sig: "0x00" },
    ];
    expect(pickLatestReceipt(set)?.receipt.to).toBe("d3");
    expect(pickLatestReceipt([])).toBeNull();
  });

  it("resolves authority to the latest receipt's `to`, falling back to primary when none", () => {
    expect(resolveAuthoritativeDevice(receipt({ to: "d9" }), "primary")).toBe("d9");
    expect(resolveAuthoritativeDevice(null, "primary")).toBe("primary");
    expect(resolveAuthoritativeDevice(null, null)).toBeNull();
  });

  it("send guard requires authority AND a signing key (§5.4)", () => {
    expect(canSend({ localDeviceId: "me", authoritativeDeviceId: "me", hasSigningKey: true })).toBe(true);
    // authoritative but read-only (no signing key) → cannot send.
    expect(canSend({ localDeviceId: "me", authoritativeDeviceId: "me", hasSigningKey: false })).toBe(false);
    // has a key but is not the authoritative device → cannot send (single-active).
    expect(canSend({ localDeviceId: "me", authoritativeDeviceId: "other", hasSigningKey: true })).toBe(false);
    // no authority resolved → cannot send.
    expect(canSend({ localDeviceId: "me", authoritativeDeviceId: null, hasSigningKey: true })).toBe(false);
  });

  it("builds a monotonically increasing next receipt", () => {
    expect(buildNextReceipt({ from: "a", to: "b", latest: null, now: 1 })).toEqual({
      v: 1,
      from: "a",
      to: "b",
      seq: 1,
      ts: 1,
    });
    expect(
      buildNextReceipt({ from: "b", to: "c", latest: receipt({ seq: 7 }), now: 99 }),
    ).toEqual({ v: 1, from: "b", to: "c", seq: 8, ts: 99 });
  });
});

function toHex(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}
