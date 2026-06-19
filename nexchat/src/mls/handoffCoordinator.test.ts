// EN: Tests for the Track A handoff coordinator (design §5.2/§5.4). The relay pointer IO layer is
// mocked with an in-memory monotone store; the directory-key crypto (derive/sign/verify) is REAL, so
// these exercise the full mint → publish → fetch+verify → resolve cycle plus forgery rejection.
// CN: 路线 A 交接协调器（设计 §5.2/§5.4）单测。relay 指针 IO 用内存单调存储 mock；目录钥密码学（派生/签/验）
// 为真，故覆盖完整 铸券→发布→取回+验签→裁决 流程及伪造拒绝。

import { beforeEach, describe, expect, it, vi } from "vitest";

import type { SyncPointer } from "@/store/syncAnchor";

const store = new Map<string, SyncPointer>();

vi.mock("@/relay/handoffPointer", () => ({
  readLocalHandoffPointer: (account: string) => store.get(account) ?? null,
  writeLocalHandoffPointer: (account: string, ptr: SyncPointer) => {
    store.set(account, ptr);
  },
  publishHandoffPointer: async (account: string, ptr: SyncPointer) => {
    const prev = store.get(account);
    if (prev && prev.updated_at > ptr.updated_at) return;
    store.set(account, ptr);
  },
  fetchHandoffPointer: async (account: string) => store.get(account) ?? null,
}));

const {
  decodeHandoffEnvelope,
  encodeHandoffEnvelope,
  fetchLatestReceipt,
  openHandoff,
  publishHandoff,
  readLocalReceipt,
  resolveAuthority,
  sealHandoff,
} = await import("@/mls/handoffCoordinator");
const { deriveDeviceDirectoryKey } = await import("@/mls/sendingAuthority");
const { endorseDevicePeerKey, generateDevicePeerKey } = await import("@/mls/devicePeerKey");

const ACCOUNT = "5Account";

beforeEach(() => store.clear());

describe("handoff envelope codec", () => {
  it("round-trips a signed receipt and rejects malformed payloads", () => {
    const signed = { receipt: { v: 1 as const, from: "a", to: "b", seq: 2, ts: 9 }, sig: "0xab" };
    expect(decodeHandoffEnvelope(encodeHandoffEnvelope(signed))).toEqual(signed);
    expect(decodeHandoffEnvelope("not-base64-json!!")).toBeNull();
    // valid base64 JSON but wrong shape → null.
    expect(decodeHandoffEnvelope(btoa(JSON.stringify({ receipt: { v: 2 }, sig: "x" })))).toBeNull();
    expect(decodeHandoffEnvelope(btoa(JSON.stringify({ sig: "x" })))).toBeNull();
  });
});

describe("mint → publish → fetch+verify → resolve (§5.2/§5.4)", () => {
  it("publishes a verifiable receipt and resolves authority to its `to`", async () => {
    const dir = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(3));
    const r = await publishHandoff({ account: ACCOUNT, dir, from: "devA", to: "devB", now: 100 });
    expect(r.receipt.seq).toBe(1);

    const latest = await fetchLatestReceipt(ACCOUNT, dir.publicKey);
    expect(latest?.receipt.to).toBe("devB");
    expect(await resolveAuthority({ account: ACCOUNT, dirPublicKey: dir.publicKey, primaryDeviceId: "devA" })).toBe(
      "devB",
    );
    expect(readLocalReceipt(ACCOUNT)?.to).toBe("devB");
  });

  it("monotonically supersedes: a second handoff bumps seq and moves authority", async () => {
    const dir = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(4));
    await publishHandoff({ account: ACCOUNT, dir, from: "devA", to: "devB", now: 100 });
    const r2 = await publishHandoff({ account: ACCOUNT, dir, from: "devB", to: "devC", now: 200 });
    expect(r2.receipt.seq).toBe(2);
    expect(await resolveAuthority({ account: ACCOUNT, dirPublicKey: dir.publicKey, primaryDeviceId: "devA" })).toBe(
      "devC",
    );
  });

  it("discards a forged receipt (wrong directory key) and falls back to primary", async () => {
    const real = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(5));
    const attacker = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(6));
    // attacker mints with its OWN key but the account verifies with the REAL directory pubkey.
    await publishHandoff({ account: ACCOUNT, dir: attacker, from: "evil", to: "evil", now: 50 });

    expect(await fetchLatestReceipt(ACCOUNT, real.publicKey)).toBeNull();
    expect(
      await resolveAuthority({ account: ACCOUNT, dirPublicKey: real.publicKey, primaryDeviceId: "devPrimary" }),
    ).toBe("devPrimary");
  });

  it("resolves to primary when no receipt exists yet (§5.1 bootstrap)", async () => {
    const dir = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(7));
    expect(
      await resolveAuthority({ account: ACCOUNT, dirPublicKey: dir.publicKey, primaryDeviceId: "devBoot" }),
    ).toBe("devBoot");
  });
});

describe("full sealed online handoff: sealHandoff → openHandoff (§5.2 steps 2–4)", () => {
  const SECRET = new Uint8Array([0xde, 0xad, 0xbe, 0xef, 1, 2, 3]);

  function captureEngine() {
    let installed: Uint8Array | null = null;
    return {
      exportSigningKeys: () => SECRET,
      installSigningKeys: (b: Uint8Array) => {
        installed = b;
      },
      get installed() {
        return installed;
      },
    };
  }

  it("hands the signing-key bundle to the endorsed recipient device", async () => {
    const dir = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(8));
    const recipient = await generateDevicePeerKey();
    const endorsement = await endorseDevicePeerKey(dir, "devNew", recipient.publicKeyRaw);

    const payload = await sealHandoff({
      account: ACCOUNT,
      dir,
      from: "devOld",
      to: "devNew",
      recipientEndorsement: endorsement,
      engine: { exportSigningKeys: () => SECRET, installSigningKeys: () => {} },
      now: 100,
    });
    // the bundle is sealed, never plaintext in the payload.
    expect(payload.sealedBundle).not.toContain("deadbeef");
    expect(payload.receipt.receipt.to).toBe("devNew");

    const newEngine = captureEngine();
    const ok = await openHandoff({
      account: ACCOUNT,
      dirPublicKey: dir.publicKey,
      myDeviceId: "devNew",
      myPeerPrivate: recipient.privateKey,
      payload,
      engine: newEngine,
    });
    expect(ok).toBe(true);
    expect(newEngine.installed).toEqual(SECRET);
  });

  it("rejects when the receipt names a different device", async () => {
    const dir = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(9));
    const recipient = await generateDevicePeerKey();
    const endorsement = await endorseDevicePeerKey(dir, "devNew", recipient.publicKeyRaw);
    const payload = await sealHandoff({
      account: ACCOUNT,
      dir,
      from: "devOld",
      to: "devNew",
      recipientEndorsement: endorsement,
      engine: { exportSigningKeys: () => SECRET, installSigningKeys: () => {} },
      now: 100,
    });
    const newEngine = captureEngine();
    const ok = await openHandoff({
      account: ACCOUNT,
      dirPublicKey: dir.publicKey,
      myDeviceId: "someoneElse",
      myPeerPrivate: recipient.privateKey,
      payload,
      engine: newEngine,
    });
    expect(ok).toBe(false);
    expect(newEngine.installed).toBeNull();
  });

  it("rejects when a different device tries to open the sealed bundle", async () => {
    const dir = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(10));
    const recipient = await generateDevicePeerKey();
    const intruder = await generateDevicePeerKey();
    const endorsement = await endorseDevicePeerKey(dir, "devNew", recipient.publicKeyRaw);
    const payload = await sealHandoff({
      account: ACCOUNT,
      dir,
      from: "devOld",
      to: "devNew",
      recipientEndorsement: endorsement,
      engine: { exportSigningKeys: () => SECRET, installSigningKeys: () => {} },
      now: 100,
    });
    const newEngine = captureEngine();
    const ok = await openHandoff({
      account: ACCOUNT,
      dirPublicKey: dir.publicKey,
      myDeviceId: "devNew",
      myPeerPrivate: intruder.privateKey,
      payload,
      engine: newEngine,
    });
    expect(ok).toBe(false);
    expect(newEngine.installed).toBeNull();
  });

  it("sealHandoff rejects an endorsement that doesn't match the target", async () => {
    const dir = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(13));
    const recipient = await generateDevicePeerKey();
    const endorsement = await endorseDevicePeerKey(dir, "devOther", recipient.publicKeyRaw);
    await expect(
      sealHandoff({
        account: ACCOUNT,
        dir,
        from: "devOld",
        to: "devNew",
        recipientEndorsement: endorsement,
        engine: { exportSigningKeys: () => SECRET, installSigningKeys: () => {} },
        now: 100,
      }),
    ).rejects.toThrow();
  });
});
