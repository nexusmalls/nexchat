// EN: Frozen shared test vectors for the EISA byte contract (ADR §5.3/§5.5). The SAME
// hex constants are asserted by `pallets/chat/sync/src/tests.rs` (cross_language_vector
// test) — a failure on either side is a contract break and a release blocker; never
// "fix the expected value" unilaterally.
// CN: EISA 字节合同的冻结共享测试向量（ADR §5.3/§5.5）。完全相同的 hex 常量由
// `pallets/chat/sync/src/tests.rs`（cross_language_vector 测试）断言——任一侧失败即合同
// 破裂、发布阻断；绝不允许单边「改期望值」。

import { beforeAll, describe, expect, it } from "vitest";
import { cryptoWaitReady, ed25519Verify } from "@polkadot/util-crypto";
import {
  buildClearPayload,
  buildPublishPayload,
  canonicalJson,
  decryptManifest,
  deriveAnchorKeys,
  deriveMlsEscrowKey,
  encryptManifest,
  signAnchorPayload,
  type AnchorKeys,
  type SyncManifest,
} from "@/store/syncAnchor";

const hex = (b: Uint8Array) => [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
const fromHex = (s: string) =>
  new Uint8Array(s.match(/.{2}/g)!.map((b) => parseInt(b, 16)));

// ---- frozen vectors (vault_master = 0x11×32, genesis = 0x22×32, ct = 0x33×48,
//      updated_at = 1738665600000) ----
const VAULT_MASTER = new Uint8Array(32).fill(0x11);
const GENESIS = new Uint8Array(32).fill(0x22);
const UPDATED_AT = 1_738_665_600_000;
const CIPHERTEXT = new Uint8Array(48).fill(0x33);
const ANCHOR_PK = "2daa51ff2538648c2e83228865a62e787c4591de51f4df34e9bc2ec51391e344";
const ANCHOR_ID = "06973db6aa8fd39ea645fe7c4ed01814905e39cb2957de9cd6eba455e2f4c2b0";
const PUBLISH_SIG =
  "9f2d8de5c58c59d70049e45706d456083b402ddf57e24e357564d1e33ef3817d8f236c371d81592e03b6830be0f3c751f31f2ccd7a6b0bb301edb8b017291b07";
const CLEAR_SIG =
  "bb32a0f667eb91615250d83b6736b586de06814274aaa498c3882cd5cb2fd960ef85a68c8dff827957ff84c7efe60b4f4d46a2e7e7ce6f799e3d876f9e518f06";

let keys: AnchorKeys;

beforeAll(async () => {
  await cryptoWaitReady();
  keys = await deriveAnchorKeys(VAULT_MASTER);
});

describe("EISA derivation (frozen vectors)", () => {
  it("derives the frozen anchor_pk / anchor_id", () => {
    expect(hex(keys.anchorPk)).toBe(ANCHOR_PK);
    expect(hex(keys.anchorId)).toBe(ANCHOR_ID);
  });

  it("is deterministic and account-separated", async () => {
    const again = await deriveAnchorKeys(VAULT_MASTER);
    expect(hex(again.anchorId)).toBe(ANCHOR_ID);
    const other = await deriveAnchorKeys(new Uint8Array(32).fill(0x12));
    expect(hex(other.anchorId)).not.toBe(ANCHOR_ID);
  });
});

describe("Track A MLS escrow key (design §4.3)", () => {
  const IV = new Uint8Array(12).fill(0x07);
  const PT = new TextEncoder().encode("mls-escrow-vault-blob");

  it("round-trips and is domain-separated from K_sync", async () => {
    const kEscrow = await deriveMlsEscrowKey(VAULT_MASTER, keys.anchorId);
    const ct = new Uint8Array(
      await crypto.subtle.encrypt({ name: "AES-GCM", iv: IV }, kEscrow, PT as BufferSource),
    );
    const back = new Uint8Array(
      await crypto.subtle.decrypt({ name: "AES-GCM", iv: IV }, kEscrow, ct as BufferSource),
    );
    expect(hex(back)).toBe(hex(PT));
    // K_sync (different salt) must NOT decrypt K_mls_escrow ciphertext.
    await expect(
      crypto.subtle.decrypt({ name: "AES-GCM", iv: IV }, keys.kSync, ct as BufferSource),
    ).rejects.toBeTruthy();
  });

  it("is deterministic and anchor/account-separated", async () => {
    const a = await deriveMlsEscrowKey(VAULT_MASTER, keys.anchorId);
    const b = await deriveMlsEscrowKey(VAULT_MASTER, keys.anchorId);
    const ct = new Uint8Array(
      await crypto.subtle.encrypt({ name: "AES-GCM", iv: IV }, a, PT as BufferSource),
    );
    // same inputs → same key: b decrypts a's ciphertext.
    const back = new Uint8Array(
      await crypto.subtle.decrypt({ name: "AES-GCM", iv: IV }, b, ct as BufferSource),
    );
    expect(hex(back)).toBe(hex(PT));
    // different anchor_id → independent key.
    const other = await deriveMlsEscrowKey(VAULT_MASTER, new Uint8Array(32).fill(0xaa));
    await expect(
      crypto.subtle.decrypt({ name: "AES-GCM", iv: IV }, other, ct as BufferSource),
    ).rejects.toBeTruthy();
  });
});

describe("signature payloads (frozen vectors)", () => {
  it("publish payload signature matches and verifies", async () => {
    const payload = await buildPublishPayload(GENESIS, keys.anchorId, UPDATED_AT, CIPHERTEXT);
    const sig = await signAnchorPayload(keys, payload);
    expect(hex(sig)).toBe(PUBLISH_SIG);
    expect(ed25519Verify(payload, sig, keys.anchorPk)).toBe(true);
  });

  it("clear payload signature matches (no ciphertext segment)", async () => {
    const payload = buildClearPayload(GENESIS, keys.anchorId, UPDATED_AT);
    const sig = await signAnchorPayload(keys, payload);
    expect(hex(sig)).toBe(CLEAR_SIG);
  });

  it("frozen signatures verify under the frozen public key (Rust mirror check)", () => {
    // Exactly what the pallet does with sp_io::crypto::ed25519_verify.
    // 与 pallet 用 sp_io::crypto::ed25519_verify 做的事完全一致。
    expect(
      ed25519Verify(
        buildClearPayload(GENESIS, fromHex(ANCHOR_ID), UPDATED_AT),
        fromHex(CLEAR_SIG),
        fromHex(ANCHOR_PK),
      ),
    ).toBe(true);
  });
});

describe("canonical JSON (frozen contract)", () => {
  it("sorts keys at every depth, drops undefined, no whitespace", () => {
    const out = canonicalJson({
      v: 1,
      updated_at: 2,
      index: { updated_at: 3, cid: "bafyA" },
      archive: undefined,
    });
    expect(out).toBe('{"index":{"cid":"bafyA","updated_at":3},"updated_at":2,"v":1}');
  });

  it("is byte-stable across property insertion order (hash-skip prerequisite)", () => {
    const a = canonicalJson({ b: 1, a: { y: 2, x: 1 } });
    const b = canonicalJson({ a: { x: 1, y: 2 }, b: 1 });
    expect(a).toBe(b);
  });
});

describe("manifest seal/open", () => {
  const manifest: SyncManifest = {
    v: 1,
    updated_at: UPDATED_AT,
    index: { cid: "bafyIndex", updated_at: UPDATED_AT },
    contacts: { cid: "bafyContacts", updated_at: UPDATED_AT - 10_000 },
  };

  it("round-trips under K_sync", async () => {
    const packed = await encryptManifest(keys, manifest);
    expect(packed.length).toBeLessThanOrEqual(512);
    const open = await decryptManifest(keys, packed);
    expect(open).toEqual(manifest);
  });

  it("another account's K_sync cannot decrypt", async () => {
    const packed = await encryptManifest(keys, manifest);
    const other = await deriveAnchorKeys(new Uint8Array(32).fill(0x12));
    await expect(decryptManifest(other, packed)).rejects.toThrow();
  });

  it("rejects manifests over the 512B chain cap", async () => {
    const fat: SyncManifest = {
      v: 1,
      updated_at: UPDATED_AT,
      index: { cid: "b".repeat(600), updated_at: UPDATED_AT },
    };
    await expect(encryptManifest(keys, fat)).rejects.toThrow(/chain cap/);
  });
});
