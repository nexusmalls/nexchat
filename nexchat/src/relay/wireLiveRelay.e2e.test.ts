// Gated behind WIRE_LIVE_E2E=1; requires relay-rs (`npm run relay:server` / cargo release).
// CN: Wire 多 leaf 在线 relay QA。WIRE_LIVE_E2E=1 门控；需 relay-rs（`npm run relay:server` / cargo release）。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Keyring } from "@polkadot/keyring";
import type { KeyringPair } from "@polkadot/keyring/types";
import { cryptoWaitReady, mnemonicGenerate } from "@polkadot/util-crypto";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { canonicalAddress } from "@/wallet/address";
import { leafKeyBindingBytes } from "@/mls/deviceLeafCredential";
import { deviceLeafIdentity, directMlsKey } from "@/mls/directConv";
import { DirectWireSession } from "@/mls/directWireSession";
import { textEnvelope } from "@/mls/envelope";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { bytesToB64 } from "@/relay/relayClient";
import { WebSocketRelay } from "@/relay/wsRelay";
import { nexchatRoot, relayReachable, spawnRelay, type SpawnedRelay } from "@/test/relayE2eSpawn";

const RUN = process.env.WIRE_LIVE_E2E === "1";
const RELAY_PORT = Number(process.env.RELAY_PORT ?? "8766");
const RELAY_URL = process.env.VITE_RELAY_WS ?? `ws://127.0.0.1:${RELAY_PORT}`;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

let spawnedRelay: SpawnedRelay | null = null;

beforeAll(async () => {
  const g = globalThis as { btoa?: (s: string) => string; atob?: (s: string) => string };
  if (!g.btoa) g.btoa = (s) => Buffer.from(s, "binary").toString("base64");
  if (!g.atob) g.atob = (s) => Buffer.from(s, "base64").toString("binary");

  const root = nexchatRoot(import.meta.url);
  spawnedRelay = await spawnRelay({ root, port: RELAY_PORT, useTempDataDir: true });
  expect(await relayReachable(RELAY_URL)).toBe(true);

  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
}, 90_000);

afterAll(() => {
  spawnedRelay?.kill();
});

describe.runIf(RUN).sequential("Wire relay live QA", () => {
  // EN: Full capability matrix runs via `scripts/npm run test:nexchat:relay-wire` (RelayTestClient).
  // Here we only live-test OpenMLS peer-assisted Add over WebSocket — the production Wire path.
  // CN: 完整能力矩阵见 `scripts/npm run test:nexchat:relay-wire`。此处仅 live 测 WebSocket 上 OpenMLS 对端代 Add。

  it(
    "peer-assisted Add converges over live WebSocket relay",
    async () => {
      await cryptoWaitReady();
      const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
      const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
      const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
      const aliceAddr = canonicalAddress(alicePair.address);
      const bobAddr = canonicalAddress(bobPair.address);
      const mlsKey = directMlsKey(aliceAddr, bobAddr);

      const aliceOld = new OpenMlsEngine();
      const aliceNew = new OpenMlsEngine();
      const bob = new OpenMlsEngine();
      await aliceOld.init(deviceLeafIdentity(aliceAddr, "old"));
      await aliceNew.init(deviceLeafIdentity(aliceAddr, "new"));
      await bob.init(deviceLeafIdentity(bobAddr, "b1"));

      const leafKey = aliceNew.signaturePublicKey();
      aliceNew.setLeafBinding(alicePair.sign(leafKeyBindingBytes(aliceAddr, "new", leafKey)));

      bob.createGroupByConv(mlsKey);
      const base = bob.addMembersByConv(mlsKey, [aliceOld.generateKeyPackage()]);
      await aliceOld.processWelcomeByConv(mlsKey, base.welcome);
      const epoch0 = bob.epochByConv(mlsKey);

      const relayBob = new WebSocketRelay(RELAY_URL);
      const relayAlice = new WebSocketRelay(RELAY_URL);
      await relayBob.connect("ep-bob-live", bobAddr);
      await relayAlice.connect("ep-alice-live", aliceAddr);
      await sleep(120);

      const bobSession = new DirectWireSession({
        engine: bob,
        relay: relayBob,
        selfAddress: bobAddr,
        deviceId: "b1",
        endpointId: "ep-bob-live",
        settleMs: 80,
      });
      const aliceSession = new DirectWireSession({
        engine: aliceNew,
        relay: relayAlice,
        selfAddress: aliceAddr,
        deviceId: "new",
        endpointId: "ep-alice-live",
        settleMs: 80,
      });
      bobSession.start();
      aliceSession.start();

      await aliceSession.requestPeerAdd(bobAddr);
      for (let i = 0; i < 30; i++) {
        if (aliceNew.hasGroup(mlsKey)) break;
        await sleep(100);
      }

      expect(aliceNew.hasGroup(mlsKey)).toBe(true);
      expect(bob.epochByConv(mlsKey)).toBe(epoch0 + 1);
      expect(aliceNew.epochByConv(mlsKey)).toBe(epoch0 + 1);

      const env = textEnvelope("wire-live-1", "wire-live", {});
      const ct = await aliceNew.encrypt(mlsKey, env);
      const plain = await bob.decrypt(mlsKey, ct);
      expect((plain.body as { text: string }).text).toBe("wire-live");
      expect(bytesToB64(ct).length).toBeGreaterThan(8);

      relayBob.disconnect();
      relayAlice.disconnect();
    },
    45_000,
  );
});
