// EN: Dual-end E2E — Bob registers inbox on WS relay, Alice blind-issues delivery tokens,
// sends a 1:1 MLS frame with sealed-sender; relay verifies RSABSSA + spent set; Bob decrypts.
// Gated behind DELIVERY_E2E=1; needs relay-rs (or auto-spawn on RELAY_PORT via `relayE2eSpawn`).
// CN: … DELIVERY_E2E=1 门控；需 relay-rs（或由 `relayE2eSpawn` 在 RELAY_PORT 自动拉起）。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { directMlsKey } from "@/mls/directConv";
import { attachDelivery, resolveInboundSender } from "@/delivery/deliveryGate";
import { InboxManager } from "@/delivery/inboxManager";
import { TokenWallet } from "@/delivery/tokenWallet";
import { TokenExchange } from "@/delivery/tokenExchange";
import { DirectMlsRegistry } from "@/mls/directMlsRegistry";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { textEnvelope } from "@/mls/envelope";
import { bytesToB64, b64ToBytes } from "@/relay/relayClient";
import { WebSocketRelay } from "@/relay/wsRelay";
import type { RelayFrame } from "@/relay/relayClient";
import { config } from "@/config";
import {
  nexchatRoot,
  relayReachable,
  spawnRelayIfNeeded,
  type SpawnedRelay,
} from "@/test/relayE2eSpawn";

const RUN = process.env.DELIVERY_E2E === "1";
const RELAY_PORT = Number(process.env.RELAY_PORT ?? "8765");
const RELAY_URL = process.env.VITE_RELAY_WS ?? `ws://127.0.0.1:${RELAY_PORT}`;

const ADDR_ALICE = "5AliceDeliveryE2E";
const ADDR_BOB = "5BobDeliveryE2ELong";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function makeMeta() {
  const m = new Map<string, unknown>();
  return {
    getMeta: async <T>(k: string) => (m.get(k) as T | undefined) ?? null,
    setMeta: async <T>(k: string, v: T) => {
      m.set(k, v);
    },
  };
}

let spawnedRelay: SpawnedRelay | null = null;

beforeAll(async () => {
  const g = globalThis as { btoa?: (s: string) => string; atob?: (s: string) => string };
  if (!g.btoa) g.btoa = (s) => Buffer.from(s, "binary").toString("base64");
  if (!g.atob) g.atob = (s) => Buffer.from(s, "base64").toString("binary");

  const root = nexchatRoot(import.meta.url);
  spawnedRelay = await spawnRelayIfNeeded({ root, port: RELAY_PORT, url: RELAY_URL });
  expect(await relayReachable(RELAY_URL)).toBe(true);

  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
}, 60_000);

afterAll(() => {
  spawnedRelay?.kill();
});

describe.runIf(RUN)("RFC 9474 delivery (dual-end WS relay)", () => {
  it(
    "Bob inbox → Alice tokens → sealed delivery frame → Bob decrypts",
    async () => {
      const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
      const uiAlice = `d:${ADDR_BOB}`;

      const bobRelay = new WebSocketRelay(RELAY_URL);
      const aliceRelay = new WebSocketRelay(RELAY_URL);
      await bobRelay.connect("ep-bob", ADDR_BOB);
      await aliceRelay.connect("ep-alice", ADDR_ALICE);

      const bobMeta = makeMeta();
      const aliceMeta = makeMeta();
      const bobInbox = new InboxManager(bobMeta);
      const aliceWallet = new TokenWallet(aliceMeta);

      await bobInbox.ensure(ADDR_BOB);
      await bobInbox.registerRelay(ADDR_BOB);
      await aliceWallet.load();

      new TokenExchange({
        selfAddress: ADDR_BOB,
        endpointId: "ep-bob",
        inbox: bobInbox,
        wallet: aliceWallet,
        relay: bobRelay,
        onNeedTokens: async () => {},
      }).wire();

      new TokenExchange({
        selfAddress: ADDR_ALICE,
        endpointId: "ep-alice",
        inbox: bobInbox,
        wallet: aliceWallet,
        relay: aliceRelay,
        onNeedTokens: async () => {},
      }).wire();

      const bobEngine = new OpenMlsEngine();
      const aliceEngine = new OpenMlsEngine();
      await bobEngine.init("bob-e2e");
      await aliceEngine.init("alice-e2e");

      const regBob = new DirectMlsRegistry({
        engine: bobEngine,
        relay: bobRelay,
        endpointId: "ep-bob",
        selfAddress: ADDR_BOB,
        onPeerStatus: () => {},
      });
      const regAlice = new DirectMlsRegistry({
        engine: aliceEngine,
        relay: aliceRelay,
        endpointId: "ep-alice",
        selfAddress: ADDR_ALICE,
        onPeerStatus: () => {},
      });
      regBob.wire();
      regAlice.wire();
      regAlice.ensure(ADDR_BOB);
      regBob.ensure(ADDR_ALICE);

      for (let i = 0; i < 60; i++) {
        if (regAlice.isReady(ADDR_BOB) && regBob.isReady(ADDR_ALICE)) break;
        await sleep(250);
      }
      expect(regAlice.isReady(ADDR_BOB)).toBe(true);
      expect(regBob.isReady(ADDR_ALICE)).toBe(true);

      await aliceWallet.requestBatch(ADDR_BOB, ADDR_ALICE, aliceRelay, "ep-alice", mlsKey);
      for (let i = 0; i < 40; i++) {
        if (aliceWallet.count(ADDR_BOB) > 0) break;
        await sleep(250);
      }
      expect(aliceWallet.count(ADDR_BOB)).toBeGreaterThan(0);

      const env = textEnvelope("e2e-1", "hello via delivery token", {});
      const ciphertext = await aliceEngine.encrypt(mlsKey, env);
      let frame: RelayFrame = {
        convId: uiAlice,
        senderRef: ADDR_ALICE,
        ciphertextB64: bytesToB64(ciphertext),
      };
      frame = await attachDelivery(frame, ADDR_BOB, ADDR_ALICE, aliceWallet);
      expect(frame.senderRef).toBe("");
      expect(frame.delivery?.t).toBeTruthy();
      expect(frame.delivery?.p).toBeTruthy();
      expect(frame.delivery?.sealedSender).toBeTruthy();
      expect(config.deliveryTokensEnabled).toBe(true);

      // EN: relay fan-out sanity (no delivery gate).
      // CN: relay 扇出冒烟（不经投递验签门）。
      const plainHit = new Promise<void>((resolve, reject) => {
        const t = setTimeout(() => reject(new Error("plain fan-out failed")), 4000);
        bobRelay.onMessage((f) => {
          if (f.dedupKey === "plain-ping") {
            clearTimeout(t);
            resolve();
          }
        });
      });
      await aliceRelay.send({
        convId: uiAlice,
        senderRef: "alice",
        ciphertextB64: bytesToB64(new Uint8Array([1, 2, 3])),
        dedupKey: "plain-ping",
      });
      await plainHit;

      // EN: sender routes with their UI id `d:{peer}` → Alice sends `d:Bob`.
      // CN: 发送方用本方 UI id `d:{peer}` 路由——Alice 发出 `d:Bob`。
      const received = new Promise<RelayFrame>((resolve, reject) => {
        const t = setTimeout(() => reject(new Error("Bob did not receive frame")), 8000);
        bobRelay.onMessage((f) => {
          if (f.convId === uiAlice && f.delivery) {
            clearTimeout(t);
            resolve(f);
          }
        });
      });
      await aliceRelay.send(frame);
      const got = await received;

      expect(got.delivery?.t).toBe(frame.delivery?.t);
      expect(frame.delivery?.mlsKey).toBe(mlsKey);
      expect(got.delivery?.mlsKey).toBe(mlsKey);
      const sender = await resolveInboundSender(got, ADDR_BOB);
      expect(sender).toBe(ADDR_ALICE);

      const plain = await bobEngine.decrypt(mlsKey, b64ToBytes(got.ciphertextB64));
      expect((plain.body as { text: string }).text).toBe("hello via delivery token");

      // EN: relay spent-set — re-send same token must not reach Bob.
      // CN: relay spent 集——重复令牌不得再次投递给 Bob。
      let dupArrived = false;
      bobRelay.onMessage((f) => {
        if (f.delivery?.t === frame.delivery?.t) dupArrived = true;
      });
      await aliceRelay.send({ ...frame, dedupKey: "dup-attempt" });
      await sleep(600);
      expect(dupArrived).toBe(false);

      bobRelay.disconnect();
      aliceRelay.disconnect();
    },
    45_000,
  );
});
