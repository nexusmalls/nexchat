// EN: Live-chain smoke test against a running Nexus dev node, now driven by the REAL
// OpenMLS (RFC 9420) WASM engine. Bob/Charlie publish real KeyPackages; Alice creates a
// group and commits adding both with a real MLS Commit + Welcome and real tree/transcript
// hashes; the new members read the Welcome back (先读后删) and process it locally. This
// proves genuine OpenMLS output is accepted by the on-chain DS/AS (§7 ordering).
// CN: 针对运行中的 Nexus dev 节点的链上冒烟测试，现由真实 OpenMLS(RFC 9420) WASM 引擎驱动。
// Bob/Charlie 发布真实 KeyPackage；Alice 建群并用真实 MLS Commit+Welcome、真实 tree/transcript
// hash 提交加人；新成员先读 Welcome 再本地处理。证明真实 OpenMLS 产物被链上 DS/AS 接受（§7）。
//
// 用法 / Usage:  node scripts/chain-smoke.mjs   (节点需运行于 ws://127.0.0.1:9944)

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { ApiPromise, WsProvider, Keyring } from "@polkadot/api";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import initWasm, { MlsClient } from "../src/mls-pkg/nexchat_mls.js";

const WS = process.env.WS || "ws://127.0.0.1:9944";
const HTTP = process.env.HTTP || "http://127.0.0.1:9944";

const hex = (u8) => "0x" + Buffer.from(u8).toString("hex");
const fromHex = (h) => Uint8Array.from(Buffer.from(h.replace(/^0x/, ""), "hex"));

let rpcId = 1;
async function rpc(method, params) {
  const res = await fetch(HTTP, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: rpcId++, method, params }),
  });
  const j = await res.json();
  if (j.error) throw new Error(`${method}: ${j.error.message}`);
  return j.result;
}

function send(api, tx, signer, label) {
  return new Promise((resolve, reject) => {
    tx.signAndSend(signer, ({ status, events, dispatchError }) => {
      if (dispatchError) {
        let msg = dispatchError.toString();
        if (dispatchError.isModule) {
          const d = api.registry.findMetaError(dispatchError.asModule);
          msg = `${d.section}.${d.name}`;
        }
        reject(new Error(`${label} failed: ${msg}`));
      } else if (status.isInBlock) {
        resolve(events.map((e) => `${e.event.section}.${e.event.method}`));
      }
    }).catch(reject);
  });
}

async function main() {
  // ---- bring up the real OpenMLS WASM engine (3 independent clients) ----
  const wasmPath = fileURLToPath(new URL("../src/mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await initWasm({ module_or_path: readFileSync(wasmPath) });
  const mlsAlice = new MlsClient("alice");
  const mlsBob = new MlsClient("bob");
  const mlsCharlie = new MlsClient("charlie");
  const cipherSuite = mlsAlice.cipherSuite();
  console.log(`\n🔐 OpenMLS ready — cipher_suite=${cipherSuite} (MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519)`);

  await cryptoWaitReady();
  const api = await ApiPromise.create({ provider: new WsProvider(WS) });
  console.log(`🔗 connected: ${await api.rpc.system.chain()}`);

  const kr = new Keyring({ type: "sr25519" });
  const alice = kr.addFromUri("//Alice");
  const bob = kr.addFromUri("//Bob");
  const charlie = kr.addFromUri("//Charlie");

  // 0) fund Bob & Charlie (deposits + fees)
  const decimals = api.registry.chainDecimals[0] ?? 12;
  const fund = 1000n * 10n ** BigInt(decimals);
  console.log(`\n⓪ fund Bob & Charlie`);
  await send(api, api.tx.balances.transferKeepAlive(bob.address, fund), alice, "fund(bob)");
  await send(api, api.tx.balances.transferKeepAlive(charlie.address, fund), alice, "fund(charlie)");

  // 1) Bob & Charlie publish REAL KeyPackages
  const bobKp = mlsBob.generateKeyPackage();
  const charlieKp = mlsCharlie.generateKeyPackage();
  console.log(`\n① publish_key_package — real KP bytes (bob ${bobKp.length}B, charlie ${charlieKp.length}B)`);
  await send(api, api.tx.chatGroup.publishKeyPackage(hex(bobKp)), bob, "publishKeyPackage(bob)");
  await send(api, api.tx.chatGroup.publishKeyPackage(hex(charlieKp)), charlie, "publishKeyPackage(charlie)");

  // 2) Alice creates the group locally (OpenMLS) → real tree/transcript hashes → on-chain
  const fp0 = mlsAlice.createGroup("g:tmp");
  console.log(`\n② create_group — real fp: tree=${hex(fp0.tree_hash).slice(0, 14)}… transcript=${hex(fp0.transcript_hash).slice(0, 14)}… epoch=${fp0.epoch}`);
  await send(
    api,
    api.tx.chatGroup.createGroup(hex(new Uint8Array([1])), cipherSuite, true, hex(fp0.tree_hash), hex(fp0.transcript_hash)),
    alice,
    "createGroup",
  );
  const groupId = (await api.query.chatGroup.nextGroupId()).toNumber() - 1;
  console.log(`   group_id = ${groupId}`);
  await send(
    api,
    api.tx.chatGroup.setGroupProfile(groupId, "OpenMLS 真实群 / Real MLS Group", null, null),
    alice,
    "setGroupProfile",
  );

  // 3) Alice commits adding [Bob, Charlie] with a REAL Commit + Welcome (1 → 3)
  const out = mlsAlice.addMembers("g:tmp", [bobKp, charlieKp]);
  console.log(`\n③ commit — real Commit ${out.commit.length}B, real Welcome ${out.welcome.length}B, new epoch ${out.epoch}`);
  await send(
    api,
    api.tx.chatGroup.commit(
      groupId,
      0,
      hex(out.commit),
      hex(out.tree_hash),
      hex(out.transcript_hash),
      hex(new Uint8Array([2])),
      // same Welcome duplicated per addee (chain wants welcome/delta bijection; each
      // member extracts its own secrets from the shared MLS Welcome)
      [
        [bob.address, hex(out.welcome)],
        [charlie.address, hex(out.welcome)],
      ],
      { added: [bob.address, charlie.address], removed: [] },
    ),
    alice,
    "commit",
  );

  // 4) New members: read Welcome back (先读), process it in OpenMLS, then claim (后删)
  console.log(`\n④ pending_welcome (read) → OpenMLS process_welcome → claim_welcome`);
  const wb = await rpc("chat_pendingWelcome", [groupId, bob.address]);
  const wc = await rpc("chat_pendingWelcome", [groupId, charlie.address]);
  mlsBob.processWelcome("g:tmp", fromHex(wb));
  mlsCharlie.processWelcome("g:tmp", fromHex(wc));
  console.log(`   bob joined group locally:     ${mlsBob.hasGroup("g:tmp")}`);
  console.log(`   charlie joined group locally: ${mlsCharlie.hasGroup("g:tmp")}`);
  await send(api, api.tx.chatGroup.claimWelcome(groupId), bob, "claimWelcome(bob)");
  await send(api, api.tx.chatGroup.claimWelcome(groupId), charlie, "claimWelcome(charlie)");

  // 5) End-to-end application message over the REAL group (off-chain payload path)
  const ct = mlsAlice.encrypt("g:tmp", new TextEncoder().encode("hello from Alice (E2EE)"));
  const dec = new TextDecoder();
  console.log(`\n⑤ application message round-trip (off-chain, real MLS AEAD)`);
  console.log(`   bob decrypts:     "${dec.decode(mlsBob.decrypt("g:tmp", ct))}"`);
  console.log(`   charlie decrypts: "${dec.decode(mlsCharlie.decrypt("g:tmp", ct))}"`);

  // 6) read-back via the chat_* RPC the frontend uses
  const snap = await rpc("chat_groupMlsSnapshot", [groupId]);
  console.log(`\n⑥ chat_groupMlsSnapshot: ${JSON.stringify(snap)}`);

  console.log("\n✅ REAL OpenMLS bytes accepted by on-chain DS/AS + E2EE round-trip OK\n");
  await api.disconnect();
}

main().catch((e) => {
  console.error("\n❌", e.message || e);
  process.exit(1);
});
