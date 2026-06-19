import { describe, expect, it } from "vitest";
import type { ControlInbound, ControlMsg, RelayClient, RelayFrame } from "@/relay/relayClient";
import {
  decodeOpkProof,
  encodeOpkProof,
  opkMerkleProof,
  opkMerkleRoot,
  verifyOpkProof,
} from "@/crypto-dr/opkMerkle";
import { MemoryDrSessionStore, type PublishedOpkBundle } from "@/crypto-dr/sessionStore";
import { OpkResponder, buildOpkLeaves, dispenseOpkLeaf, requestOpk } from "@/crypto-dr/opkExchange";

const toHex = (b: Uint8Array): string =>
  Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

const mkOpks = (n: number): Uint8Array[] =>
  Array.from({ length: n }, (_, i) => new Uint8Array(32).fill(i + 1));

/// Loopback control hub — `sendControl` fans to every OTHER endpoint's `onControl`.
class ControlHub {
  private subs: Array<{ ref: string; cb: ControlInbound }> = [];
  endpoint(ref: string): RelayClient {
    const hub = this;
    return {
      async connect() {},
      async send(_f: RelayFrame) {},
      onMessage() {},
      async sendControl(msg: ControlMsg) {
        for (const s of hub.subs) if (s.ref !== ref) s.cb(msg);
      },
      onControl(cb: ControlInbound) {
        hub.subs.push({ ref, cb });
      },
      disconnect() {},
    };
  }
}

describe("opkMerkle — proof codec (relay control-plane transport, §21)", () => {
  it("round-trips a proof and still verifies against the root", () => {
    const opks = mkOpks(5);
    const root = opkMerkleRoot(opks);
    const proof = opkMerkleProof(opks, opks[2]!);
    const decoded = decodeOpkProof(encodeOpkProof(proof));
    expect(decoded).toEqual(proof);
    expect(verifyOpkProof(root, opks[2]!, decoded)).toBe(true);
  });

  it("rejects a wrong byte length", () => {
    expect(() => decodeOpkProof(new Uint8Array(32))).toThrow();
  });
});

describe("dispenseOpkLeaf — single-dispense with spent-set (§19)", () => {
  it("serves each leaf once, then exhausts", () => {
    const opks = mkOpks(3);
    const root = opkMerkleRoot(opks);
    let bundle: PublishedOpkBundle = {
      device: "dev",
      root: toHex(root),
      opks: opks.map(toHex),
      spent: [],
    };
    const served = new Set<string>();
    for (let i = 0; i < 3; i++) {
      const out = dispenseOpkLeaf(bundle)!;
      expect(out).not.toBeNull();
      served.add(out.leaf.opk_pub);
      // proof verifies against the on-chain root
      const opk = opks.find((k) => toHex(k) === out.leaf.opk_pub)!;
      expect(verifyOpkProof(root, opk, decodeOpkProof(b64(out.leaf.proof)))).toBe(true);
      bundle = out.bundle;
    }
    expect(served.size).toBe(3); // all distinct
    expect(dispenseOpkLeaf(bundle)).toBeNull(); // exhausted
  });
});

describe("buildOpkLeaves — relay upload payload (OPK-over-relay §19/§21)", () => {
  it("emits a verifiable proof for every UNSPENT leaf and skips spent ones", () => {
    const opks = mkOpks(4);
    const root = opkMerkleRoot(opks);
    const bundle: PublishedOpkBundle = {
      device: "dev",
      root: toHex(root),
      opks: opks.map(toHex),
      spent: [toHex(opks[1]!)],
    };
    const leaves = buildOpkLeaves(bundle);
    expect(leaves).toHaveLength(3); // 4 total - 1 spent
    expect(leaves.map((l) => l.opk_pub)).not.toContain(toHex(opks[1]!));
    for (const l of leaves) {
      const opk = opks.find((k) => toHex(k) === l.opk_pub)!;
      expect(verifyOpkProof(root, opk, decodeOpkProof(b64(l.proof)))).toBe(true);
    }
  });

  it("returns [] when the set is fully spent", () => {
    const opks = mkOpks(2);
    expect(
      buildOpkLeaves({ device: "d", root: "00", opks: opks.map(toHex), spent: opks.map(toHex) }),
    ).toEqual([]);
  });
});

describe("OpkResponder.upload + serve toAddr (OPK-over-relay §19/§21)", () => {
  it("uploads the unspent leaf set on its self channel for relay caching", async () => {
    const opks = mkOpks(3);
    const root = opkMerkleRoot(opks);
    const deviceHex = "abcd1234";
    const store = new MemoryDrSessionStore();
    await store.saveOpkBundle({ device: deviceHex, root: toHex(root), opks: opks.map(toHex), spent: [] });

    const seen: ControlMsg[] = [];
    const relay: RelayClient = {
      async connect() {},
      async send() {},
      onMessage() {},
      async sendControl(msg) {
        seen.push(msg);
      },
      onControl() {},
      disconnect() {},
    };
    await new OpkResponder(relay, store, "5bob", deviceHex).upload();

    expect(seen).toHaveLength(1);
    const msg = seen[0]!;
    expect(msg.t).toBe("opk_publish");
    if (msg.t !== "opk_publish") throw new Error("unreachable");
    expect(msg.convId).toBe("s:5bob");
    expect(msg.device_id).toBe(deviceHex);
    expect(msg.leaves).toHaveLength(3);
    expect(msg.toAddr).toBeUndefined(); // bulk advertisement → relay caches (no routing)
  });

  it("stamps toAddr (the fetch initiator) on a live single-leaf reply", async () => {
    const opks = mkOpks(2);
    const root = opkMerkleRoot(opks);
    const deviceHex = "feedface";
    const store = new MemoryDrSessionStore();
    await store.saveOpkBundle({ device: deviceHex, root: toHex(root), opks: opks.map(toHex), spent: [] });

    const replies: ControlMsg[] = [];
    let fetchCb: ControlInbound | null = null;
    const relay: RelayClient = {
      async connect() {},
      async send() {},
      onMessage() {},
      async sendControl(msg) {
        replies.push(msg);
      },
      onControl(cb) {
        fetchCb = cb;
      },
      disconnect() {},
    };
    const responder = new OpkResponder(relay, store, "5bob", deviceHex);
    responder.attach();
    fetchCb!({ t: "opk_fetch", convId: "s:5bob", from: "5alice", target_device: deviceHex });
    await Promise.resolve();
    await Promise.resolve();

    expect(replies).toHaveLength(1);
    const msg = replies[0]!;
    if (msg.t !== "opk_publish") throw new Error("expected opk_publish");
    expect(msg.toAddr).toBe("5alice"); // routes the live reply back to the initiator
    expect(msg.leaves).toHaveLength(1);
  });
});

describe("requestOpk ⇄ OpkResponder over the control plane", () => {
  it("fetches a verified one-time prekey and single-dispenses", async () => {
    const opks = mkOpks(2);
    const root = opkMerkleRoot(opks);
    const deviceHex = "abcd1234";
    const store = new MemoryDrSessionStore();
    await store.saveOpkBundle({ device: deviceHex, root: toHex(root), opks: opks.map(toHex), spent: [] });

    const hub = new ControlHub();
    const responder = new OpkResponder(hub.endpoint("bob"), store, "bob", deviceHex);
    responder.attach();

    const peerDevice = hexToBytes(deviceHex);
    const got1 = await requestOpk(hub.endpoint("alice"), {
      selfRef: "alice",
      peerAccount: "5bob",
      peerDevice,
      root,
      timeoutMs: 1000,
    });
    expect(got1).not.toBeNull();
    expect(verifyOpkProof(root, got1!, opkMerkleProof(opks, got1!))).toBe(true);

    const got2 = await requestOpk(hub.endpoint("alice2"), {
      selfRef: "alice2",
      peerAccount: "5bob",
      peerDevice,
      root,
      timeoutMs: 1000,
    });
    expect(got2).not.toBeNull();
    expect(toHex(got2!)).not.toBe(toHex(got1!)); // single-dispense → distinct leaf

    // exhausted → next request times out (SPK fallback)
    const got3 = await requestOpk(hub.endpoint("alice3"), {
      selfRef: "alice3",
      peerAccount: "5bob",
      peerDevice,
      root,
      timeoutMs: 120,
    });
    expect(got3).toBeNull();
  });

  it("times out (returns null) when no responder is online", async () => {
    const hub = new ControlHub();
    const got = await requestOpk(hub.endpoint("alice"), {
      selfRef: "alice",
      peerAccount: "5bob",
      peerDevice: new Uint8Array(16).fill(9),
      root: new Uint8Array(32),
      timeoutMs: 120,
    });
    expect(got).toBeNull();
  });
});

function b64(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}
