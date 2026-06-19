import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import type { ChainClient } from "@/chain/chainClient";
import { ensureDrWasm } from "@/crypto-dr/vodozemacEngine";
import { MemoryConvStackRegistry } from "@/crypto-dr/convStack";
import { MemoryDrSessionStore } from "@/crypto-dr/sessionStore";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import type { RelayClient, RelayFrame } from "@/relay/relayClient";
import type { ArchivePusher } from "@/orchestrator/archiveAdapter";
import { assembleChatStack } from "@/orchestrator/assembleChatStack";

function noopRelay(): RelayClient {
  return {
    async connect() {},
    async send(_f: RelayFrame) {},
    onMessage() {},
    async sendControl() {},
    onControl() {},
    disconnect() {},
  };
}

let aliceAddr: string;

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../dr-pkg/nexchat_dr_bg.wasm", import.meta.url));
  await ensureDrWasm(readFileSync(wasmPath));
  await cryptoWaitReady();
  const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
  aliceAddr = kr.addFromUri("//Alice").address;
});

describe("assembleChatStack — app-startup DR + orchestrator composition", () => {
  it("builds engine, transport, router, orchestrator and pins via the registry", async () => {
    const archivePusher: ArchivePusher = { push: vi.fn().mockResolvedValue(undefined) };
    const fakeMls = { hasGroup: () => false } as unknown as OpenMlsEngine;
    const fakeChain = {} as ChainClient;
    const drStore = new MemoryDrSessionStore();
    const stackRegistry = new MemoryConvStackRegistry();

    const stack = await assembleChatStack({
      account: aliceAddr,
      relay: noopRelay(),
      chain: fakeChain,
      mlsEngine: fakeMls,
      endpointId: "ep-test",
      archivePusher,
      drStore,
      stackRegistry,
    });

    expect(stack.engine.deviceId().length).toBe(16); // blake2_128(IK)
    expect(stack.transport).toBeDefined();
    expect(stack.router).toBeDefined();
    expect(stack.orchestrator.getMode("d:5x")).toBe("none");
    expect(stack.stackRegistry).toBe(stackRegistry);
    expect(stack.opkResponder).toBeDefined(); // OPK responder attached to the control plane

    // engine identity persisted to the injected store (restart survivability)
    expect(await drStore.loadAccount()).toBeTruthy();
  });

  it("restores the same device identity from a populated store (restart)", async () => {
    const drStore = new MemoryDrSessionStore();
    const base = {
      relay: noopRelay(),
      chain: {} as ChainClient,
      mlsEngine: { hasGroup: () => false } as unknown as OpenMlsEngine,
      endpointId: "ep",
      archivePusher: { push: vi.fn().mockResolvedValue(undefined) } as ArchivePusher,
      drStore,
    };
    const first = await assembleChatStack({ account: aliceAddr, ...base });
    const second = await assembleChatStack({ account: aliceAddr, ...base });
    expect(second.engine.deviceId()).toEqual(first.engine.deviceId());
  });
});
