// EN: H6 whole-chain E2E for the GROUP (Track A) multi-device story (design §10): a member swaps to a
// new device, the new device restores the group READ-ONLY from the escrow vault and reads history, then
// an ONLINE HANDOFF transfers sending authority so the new device can post to the group. Unlike 1:1
// Wire (per-device leaf), a group uses ONE account leaf with the signing key handed device→device; this
// stitches the real `OpenMlsEngine` escrow restore together with the real `handoffCoordinator` (which is
// otherwise unit-tested against a MOCK engine). CN: 群（路线 A）多设备故事的 H6 整链 E2E（设计 §10）：成员换到
// 新设备，新设备从托管 vault **只读**恢复群并读历史，再经**在线交接**把发送权交给新设备使其可在群里发言。与
// 1:1 Wire（每设备一 leaf）不同，群用**单一账户 leaf**、签名钥逐设备交接；本测试把真实 `OpenMlsEngine` 托管恢复
// 与真实 `handoffCoordinator`（平时只对 MOCK 引擎单测）缝合为整链。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import type { SyncPointer } from "@/store/syncAnchor";

// EN: in-memory monotone pointer store so the handoff receipt publish/fetch is real (only the relay IO
// is mocked). CN: 内存单调指针存储，使交接回执的发布/取回为真（仅 mock relay IO）。
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

const { OpenMlsEngine } = await import("@/mls/openMlsEngine");
const { deriveDeviceDirectoryKey } = await import("@/mls/sendingAuthority");
const { generateDevicePeerKey, endorseDevicePeerKey } = await import("@/mls/devicePeerKey");
const { sealHandoff, openHandoff, resolveAuthority } = await import("@/mls/handoffCoordinator");
const { textEnvelope } = await import("@/mls/envelope");

const GROUP = "g:team";
const ALICE = "5AliceAccount";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

beforeEach(() => store.clear());

describe("H6 group device-swap read + online handoff (Track A, whole-chain)", () => {
  it("a new device restores read-only from escrow, reads history, then gains send authority via online handoff", async () => {
    // ── 3-member group (≥3 avoids TwoMemberGroupForbidden): alice(creator) + bob + carol ──────────
    const aliceOld = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    const carol = new OpenMlsEngine();
    await aliceOld.init(ALICE);
    await bob.init("5BobAccount");
    await carol.init("5CarolAccount");

    aliceOld.createGroupByConv(GROUP);
    const add = aliceOld.addMembersByConv(GROUP, [
      bob.generateKeyPackage(),
      carol.generateKeyPackage(),
    ]);
    await bob.processWelcomeByConv(GROUP, add.welcome);
    await carol.processWelcomeByConv(GROUP, add.welcome);

    // baseline history everyone reads (advances alice's own + received ratchets before the snapshot).
    const m1 = await aliceOld.encrypt(GROUP, textEnvelope("m1", "kickoff", {}));
    expect((await bob.decrypt(GROUP, m1)).body).toMatchObject({ text: "kickoff" });
    expect((await carol.decrypt(GROUP, m1)).body).toMatchObject({ text: "kickoff" });
    const m2 = await bob.encrypt(GROUP, textEnvelope("m2", "ack", {}));
    expect((await aliceOld.decrypt(GROUP, m2)).body).toMatchObject({ text: "ack" });
    expect((await carol.decrypt(GROUP, m2)).body).toMatchObject({ text: "ack" });
    const epoch = aliceOld.epochByConv(GROUP);

    // ── 换机读 (device-swap read): alice's NEW device cold-restores READ-ONLY from the escrow vault ─
    expect(aliceOld.canExportEscrow()).toBe(true);
    const vault = aliceOld.exportEscrowState();

    const aliceNew = new OpenMlsEngine();
    await aliceNew.init(ALICE); // cold-start full client …
    expect(aliceNew.groupCount()).toBe(0);
    aliceNew.importEscrowVault(vault); // … replaced by the READ-ONLY restored client
    expect(aliceNew.canExportEscrow()).toBe(false); // no signer → read-only
    expect(aliceNew.epochByConv(GROUP)).toBe(epoch);

    // it reads current-epoch traffic: a fresh peer message decrypts on the new device.
    const m3 = await carol.encrypt(GROUP, textEnvelope("m3", "welcome new device", {}));
    expect((await aliceNew.decrypt(GROUP, m3)).body).toMatchObject({ text: "welcome new device" });

    // but a read-only device CANNOT send before the handoff (no signing key).
    await expect(aliceNew.encrypt(GROUP, textEnvelope("x", "nope", {}))).rejects.toBeTruthy();

    // ── 在线交接 (online handoff): old device seals signing authority → new device opens & installs ─
    const dir = await deriveDeviceDirectoryKey(new Uint8Array(32).fill(11));
    const recipient = await generateDevicePeerKey();
    const endorsement = await endorseDevicePeerKey(dir, "devNew", recipient.publicKeyRaw);

    // bootstrap authority is the old device; once a receipt is published it moves to devNew.
    expect(
      await resolveAuthority({ account: ALICE, dirPublicKey: dir.publicKey, primaryDeviceId: "devOld" }),
    ).toBe("devOld");

    const payload = await sealHandoff({
      account: ALICE,
      dir,
      from: "devOld",
      to: "devNew",
      recipientEndorsement: endorsement,
      engine: aliceOld, // exports the REAL signing-key bundle, sealed to the recipient peer key
      now: 100,
    });
    // the bundle never travels in the clear.
    expect(payload.sealedBundle.length).toBeGreaterThan(0);

    const ok = await openHandoff({
      account: ALICE,
      dirPublicKey: dir.publicKey,
      myDeviceId: "devNew",
      myPeerPrivate: recipient.privateKey,
      payload,
      engine: aliceNew, // installs the signing keys → upgrades the read-only client to a sender
    });
    expect(ok).toBe(true);

    // authority now resolves to the new device, and it is no longer read-only.
    expect(
      await resolveAuthority({ account: ALICE, dirPublicKey: dir.publicKey, primaryDeviceId: "devOld" }),
    ).toBe("devNew");
    expect(aliceNew.canExportEscrow()).toBe(true);

    // ── the new device now SENDS to the group as alice; both peers decrypt (continuity, no rekey) ──
    const m4 = await aliceNew.encrypt(GROUP, textEnvelope("m4", "sending from my new phone", {}));
    expect((await bob.decrypt(GROUP, m4)).body).toMatchObject({ text: "sending from my new phone" });
    expect((await carol.decrypt(GROUP, m4)).body).toMatchObject({ text: "sending from my new phone" });
  });
});
