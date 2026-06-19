// EN: LIVE end-to-end test for the on-chain DS/AS handshake control-plane. Runs three real
// coordinators (Alice=owner, Bob, Charlie) against a running `nexus-node --dev`: owner funds
// + creates the group + commits Adds; members publish KeyPackages, discover the group, claim
// their Welcome. Then we prove a real OpenMLS application message round-trips and that a
// member's state survives export → restore (the persistence guarantee).
// Gated behind CHAIN_E2E=1 (and a reachable node) so it never runs in normal CI.
// CN: 链上 DS/AS 握手控制面的**实时**端到端测试：对运行中的 `nexus-node --dev` 跑三个真实协调器
// （Alice=owner，Bob，Charlie）：owner 转账+建群+commit 加人；成员发布 KeyPackage、发现群、领取
// Welcome。随后验证真实 OpenMLS 应用消息往返，且成员状态经 export→restore 仍可用（持久化保证）。
// 用 CHAIN_E2E=1（且节点可达）门控，正常 CI 不会触发。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import init, { MlsClient } from "../mls-pkg/nexchat_mls.js";
import { ChainClient } from "@/chain/chainClient";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { ChainMlsCoordinator } from "@/mls/chainHandshake";
import { textEnvelope } from "@/mls/envelope";

const RUN = process.env.CHAIN_E2E === "1";
// EN: ALL participants use FRESH per-run seeds so the chain starts with zero stale state
// (no KeyPackages, no group-creation cooldown). //Alice (genesis-funded) bootstraps the owner,
// then the owner funds the members from zero — the real flow. CN: 全部参与者用每次运行全新种子，
// 链上无残留状态（无 KeyPackage、无建群冷却）。//Alice（创世发币）先给 owner 充值，owner 再从零
// 给成员充值——真实流程。
const STAMP = Date.now();
const OWNER = `//nexchat-e2e-${STAMP}-owner`;
const ROSTER = [OWNER, `//nexchat-e2e-${STAMP}-b`, `//nexchat-e2e-${STAMP}-c`];
const BOOTSTRAP = 500_000_000_000_000n; // 500 NEX to the owner (group deposit + funds members + fees)
const dec = new TextDecoder();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function nodeUp(): Promise<boolean> {
  try {
    const res = await fetch("http://127.0.0.1:9944", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "system_chain", params: [] }),
    });
    return res.ok;
  } catch {
    return false;
  }
}

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

describe.runIf(RUN)("chain DS/AS handshake (live node)", () => {
  it(
    "owner mints group, members join on-chain, OpenMLS message round-trips, state persists",
    async () => {
      expect(await nodeUp()).toBe(true);

      // Bootstrap: //Alice (the genesis-funded account) endows the fresh owner so it can pay
      // the group deposit and fund the members.
      const faucet = new ChainClient();
      await faucet.useDevAccount("//Alice");
      const ownerAddr = await faucet.deriveAddress(OWNER);
      await faucet.signAndSendDev("balances", "transferKeepAlive", [ownerAddr, BOOTSTRAP]);

      // Build one independent participant (own ChainClient + OpenMLS engine + coordinator).
      const groupIds: Record<string, number | null> = {};
      const errors: string[] = [];
      const tabs = await Promise.all(
        ROSTER.map(async (seed) => {
          const chain = new ChainClient();
          const self = await chain.useDevAccount(seed);
          const engine = new OpenMlsEngine();
          await engine.init(self); // no persistKey: IndexedDB is absent in Node anyway
          const roster = await Promise.all(ROSTER.map((s) => chain.deriveAddress(s)));
          const coord = new ChainMlsCoordinator({
            engine,
            chain,
            selfAddress: self,
            roster,
            pollMs: 1500,
            onStatus: () => undefined,
            onGroupId: (gid) => (groupIds[self] = gid),
            onError: (e) => errors.push(`${seed}: ${e}`),
          });
          return { seed, self, engine, coord };
        }),
      );

      tabs.forEach((t) => t.coord.start());

      // Wait for every tab to be holding the (same) group.
      const deadline = Date.now() + 120_000;
      let converged = false;
      while (Date.now() < deadline) {
        const gids = tabs.map((t) => groupIds[t.self]);
        const ready =
          gids.every((g) => g != null) &&
          new Set(gids).size === 1 &&
          tabs.every((t) => t.engine.hasGroup(`g:${gids[0]}`));
        if (ready) {
          converged = true;
          break;
        }
        await sleep(1500);
      }
      tabs.forEach((t) => t.coord.stop());

      expect(errors, errors.join("\n")).toEqual([]);
      expect(converged).toBe(true);

      const gid = groupIds[tabs[0].self] as number;
      const [alice, bob, charlie] = tabs;

      // Real OpenMLS application message: owner → group, both members decrypt.
      const env = textEnvelope("m1", "hello from the chain handshake");
      const ct = await alice.engine.encrypt(`g:${gid}`, env);
      expect((await bob.engine.decrypt(`g:${gid}`, ct)).body).toEqual(env.body);
      expect((await charlie.engine.decrypt(`g:${gid}`, ct)).body).toEqual(env.body);

      // Persistence: snapshot Bob, rebuild a fresh client, keep decrypting on the SAME ratchet.
      const blob = (bob.engine as unknown as { client: MlsClient }).client.exportState();
      const bob2 = MlsClient.restore(blob);
      const ct2 = await alice.engine.encrypt(`g:${gid}`, textEnvelope("m2", "after refresh"));
      expect(dec.decode(bob2.decrypt(`g:${gid}`, ct2))).toContain("after refresh");
    },
    150_000,
  );
});
