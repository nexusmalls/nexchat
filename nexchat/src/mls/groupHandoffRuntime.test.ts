// EN: Tests for the Track A group send-authority RUNTIME (design §5.2/§7.3) — the application glue that
// drives the online signing-key handoff over the account self-channel. The relay pointer IO is mocked
// (in-memory monotone store) and the device peer key is a single generated keypair; the directory-key
// crypto + seal/open are REAL. Covers: mode resolution (primary / secondary / no-vault), request frame
// emission, the OLD-primary grant path, and the NEW-device install path.
// CN: 路线 A 群发送权**运行时**（设计 §5.2/§7.3）单测——驱动账户自通道在线签名钥交接的应用胶水。relay 指针 IO
// 用内存单调存储 mock、设备对端钥为单一生成密钥对；目录钥密码学 + seal/open 为真。覆盖：态解析（primary /
// secondary / 无 vault）、请求帧发出、旧主授权路径、新设备装入路径。

import { beforeEach, describe, expect, it, vi } from "vitest";

import type { SyncPointer } from "@/store/syncAnchor";
import type { ControlMsg, RelayClient } from "@/relay/relayClient";

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

// EN: the runtime's device peer key comes from a single lazily-generated keypair (no localStorage in
// tests); the rest of devicePeerKey (seal/open/endorse) stays real. CN: 运行时设备对端钥来自单一惰性生成
// 密钥对（测试无 localStorage）；devicePeerKey 其余（seal/open/endorse）保持真实。
vi.mock("@/mls/devicePeerKey", async (importActual) => {
  const actual = await importActual<typeof import("@/mls/devicePeerKey")>();
  let cached: Awaited<ReturnType<typeof actual.generateDevicePeerKey>> | null = null;
  return {
    ...actual,
    getOrCreateDevicePeerKey: async () => {
      if (!cached) cached = await actual.generateDevicePeerKey();
      return cached;
    },
  };
});

const { generateDevicePeerKey, endorseDevicePeerKey } = await import("@/mls/devicePeerKey");
const { deriveDeviceDirectoryKey } = await import("@/mls/sendingAuthority");
const { sealHandoff } = await import("@/mls/handoffCoordinator");
const { GroupHandoffRuntime } = await import("@/mls/groupHandoffRuntime");

const ACCOUNT = "5Account";
const MASTER = new Uint8Array(32).fill(7);
const SECRET = new Uint8Array([1, 2, 3, 4, 5]);

function fakeRelay() {
  const sent: ControlMsg[] = [];
  let handler: ((m: ControlMsg) => void) | null = null;
  const relay = {
    onControl: (cb: (m: ControlMsg) => void) => {
      handler = cb;
    },
    sendControl: async (m: ControlMsg) => {
      sent.push(m);
    },
  } as unknown as RelayClient;
  return { relay, sent, dispatch: async (m: ControlMsg) => await handler?.(m) };
}

function fullEngine() {
  let hasKey = true;
  return {
    canExportEscrow: () => hasKey,
    exportSigningKeys: () => SECRET,
    installSigningKeys: () => {
      hasKey = true;
    },
  };
}

function readonlyEngine() {
  let hasKey = false;
  let installed: Uint8Array | null = null;
  return {
    canExportEscrow: () => hasKey,
    exportSigningKeys: (): Uint8Array => {
      throw new Error("read-only");
    },
    installSigningKeys: (b: Uint8Array) => {
      installed = b;
      hasKey = true;
    },
    get installed() {
      return installed;
    },
  };
}

beforeEach(() => store.clear());

describe("GroupHandoffRuntime mode resolution", () => {
  it("no vault_master → stays primary (single-device behaviour)", async () => {
    const { relay } = fakeRelay();
    const rt = new GroupHandoffRuntime();
    await rt.start({ account: ACCOUNT, selfDeviceId: "d1", relay, engine: fullEngine(), vaultMaster: null });
    expect(rt.mode()).toBe("primary");
    expect(rt.canSend()).toBe(true);
  });

  it("full client with no receipt → primary (§5.1 bootstrap)", async () => {
    const { relay } = fakeRelay();
    const rt = new GroupHandoffRuntime();
    await rt.start({ account: ACCOUNT, selfDeviceId: "old", relay, engine: fullEngine(), vaultMaster: MASTER });
    expect(rt.mode()).toBe("primary");
    expect(rt.canSend()).toBe(true);
  });

  it("read-only client with no receipt → secondary (must request authority)", async () => {
    const { relay } = fakeRelay();
    const rt = new GroupHandoffRuntime();
    await rt.start({ account: ACCOUNT, selfDeviceId: "new", relay, engine: readonlyEngine(), vaultMaster: MASTER });
    expect(rt.mode()).toBe("secondary");
    expect(rt.canSend()).toBe(false);
  });
});

describe("GroupHandoffRuntime handoff flow", () => {
  it("read-only device emits a verifiable handoff-request and installs on grant", async () => {
    const { relay, sent, dispatch } = fakeRelay();
    const engine = readonlyEngine();
    const rt = new GroupHandoffRuntime();
    await rt.start({ account: ACCOUNT, selfDeviceId: "new", relay, engine, vaultMaster: MASTER });

    await rt.requestSendAuthority();
    const req = sent.find((m) => m.t === "handoff-request");
    expect(req).toBeTruthy();
    if (req?.t !== "handoff-request") throw new Error("no request");
    expect(req.from).toBe("new");
    expect(req.convId).toBe(`s:${ACCOUNT}`);

    // simulate the OLD primary granting authority using the request's endorsement.
    const dir = await deriveDeviceDirectoryKey(MASTER);
    const payload = await sealHandoff({
      account: ACCOUNT,
      dir,
      from: "old",
      to: "new",
      recipientEndorsement: req.endorsement,
      engine: { exportSigningKeys: () => SECRET, installSigningKeys: () => {} },
      now: 100,
    });
    await dispatch({ t: "handoff-grant", convId: `s:${ACCOUNT}`, to: "new", payload });

    expect(engine.installed).toEqual(SECRET);
    expect(rt.canSend()).toBe(true);
    expect(rt.mode()).toBe("primary");
  });

  it("old primary grants authority on a valid request and then drops to secondary", async () => {
    const { relay, sent, dispatch } = fakeRelay();
    const engine = fullEngine();
    const rt = new GroupHandoffRuntime();
    await rt.start({ account: ACCOUNT, selfDeviceId: "old", relay, engine, vaultMaster: MASTER });
    expect(rt.canSend()).toBe(true);

    // a sibling read-only device requests authority (endorsed with the same directory key).
    const dir = await deriveDeviceDirectoryKey(MASTER);
    const recipient = await generateDevicePeerKey();
    const endorsement = await endorseDevicePeerKey(dir, "new", recipient.publicKeyRaw);
    await dispatch({ t: "handoff-request", convId: `s:${ACCOUNT}`, from: "new", endorsement });

    const grant = sent.find((m) => m.t === "handoff-grant");
    expect(grant).toBeTruthy();
    if (grant?.t !== "handoff-grant") throw new Error("no grant");
    expect(grant.to).toBe("new");
    // authority moved to the new device → the old primary can no longer send.
    expect(rt.canSend()).toBe(false);
    expect(rt.mode()).toBe("secondary");
  });

  it("ignores a grant addressed to a different device", async () => {
    const { relay, sent, dispatch } = fakeRelay();
    const engine = readonlyEngine();
    const rt = new GroupHandoffRuntime();
    await rt.start({ account: ACCOUNT, selfDeviceId: "new", relay, engine, vaultMaster: MASTER });
    await rt.requestSendAuthority();
    const req = sent.find((m) => m.t === "handoff-request");
    if (req?.t !== "handoff-request") throw new Error("no request");

    const dir = await deriveDeviceDirectoryKey(MASTER);
    const payload = await sealHandoff({
      account: ACCOUNT,
      dir,
      from: "old",
      to: "new",
      recipientEndorsement: req.endorsement,
      engine: { exportSigningKeys: () => SECRET, installSigningKeys: () => {} },
      now: 100,
    });
    await dispatch({ t: "handoff-grant", convId: `s:${ACCOUNT}`, to: "someoneElse", payload });

    expect(engine.installed).toBeNull();
    expect(rt.canSend()).toBe(false);
  });
});
