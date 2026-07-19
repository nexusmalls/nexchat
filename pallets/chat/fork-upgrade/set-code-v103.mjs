#!/usr/bin/env node
/**
 * EN: Submit sudo.setCode(v103 wasm) on a local Chopsticks fork (default ws://127.0.0.1:8000).
 * CN: 在本地 Chopsticks fork 上提交 sudo.setCode(v103 wasm)。
 *
 * Usage / 用法:
 *   cd pallets/chat/fork-upgrade && npm install
 *   SUDO_URI='your twelve or twenty-four words...' npm run set-code
 *
 * Optional env / 可选环境变量:
 *   WS_URL=ws://127.0.0.1:8000
 *   WASM=../../../target/release/wbuild/nexus-runtime/nexus_runtime.compact.compressed.wasm
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { ApiPromise, WsProvider } from "@polkadot/api";
import { Keyring } from "@polkadot/keyring";
import { u8aToHex } from "@polkadot/util";
import { cryptoWaitReady } from "@polkadot/util-crypto";

const here = dirname(fileURLToPath(import.meta.url));
const WS_URL = process.env.WS_URL ?? "ws://127.0.0.1:8000";
const WASM = resolve(
  here,
  process.env.WASM ??
    "../../../target/release/wbuild/nexus-runtime/nexus_runtime.compact.compressed.wasm",
);
const SUDO_URI = process.env.SUDO_URI ?? process.env.SUDO_SEED ?? "";

if (!SUDO_URI) {
  console.error(
    "缺少 SUDO_URI（主网 Sudo 账户助记词或 //Alice 等 SURI）。\n" +
      "Missing SUDO_URI (mainnet sudo mnemonic or //Alice SURI).\n\n" +
      "示例 Example:\n" +
      "  SUDO_URI='word1 word2 ...' npm run set-code",
  );
  process.exit(1);
}

const wasm = readFileSync(WASM);
console.log(`WASM: ${WASM} (${wasm.length} bytes)`);
console.log(`RPC:  ${WS_URL}`);

await cryptoWaitReady();
const api = await ApiPromise.create({ provider: new WsProvider(WS_URL) });

try {
  const chain = await api.rpc.system.chain();
  const before = (await api.rpc.state.getRuntimeVersion()).specVersion.toNumber();
  console.log(`Chain: ${chain}, specVersion before: ${before}`);

  const sudoKey = (await api.query.sudo.key()).toString();
  console.log(`On-chain sudo account: ${sudoKey}`);

  const keyring = new Keyring({ type: "sr25519", ss58Format: 273 });
  const signer = keyring.addFromUri(SUDO_URI);
  if (signer.address !== sudoKey) {
    console.error(
      `SUDO_URI 对应地址 ${signer.address} 与链上 sudo ${sudoKey} 不一致。\n` +
        `Signer ${signer.address} != on-chain sudo ${sudoKey}`,
    );
    process.exit(1);
  }

  // Polkadot.js expects setCode argument as 0x-prefixed hex, not a raw Buffer.
  const wasmHex = u8aToHex(wasm);
  const tx = api.tx.sudo.sudo(api.tx.system.setCode(wasmHex));
  console.log("Submitting sudo(setCode)...");

  await new Promise((resolvePromise, reject) => {
    tx.signAndSend(signer, ({ status, dispatchError, events }) => {
      if (status.isInBlock || status.isFinalized) {
        if (dispatchError) {
          if (dispatchError.isModule) {
            const meta = api.registry.findMetaError(dispatchError.asModule);
            reject(new Error(`${meta.section}.${meta.name}`));
          } else {
            reject(new Error(dispatchError.toString()));
          }
          return;
        }
        for (const { event } of events) {
          if (api.events.system.ExtrinsicSuccess.is(event)) {
            console.log(`ExtrinsicSuccess in ${status.isFinalized ? "finalized" : "in-block"}`);
          }
        }
        resolvePromise();
      }
    }).catch(reject);
  });

  const after = (await api.rpc.state.getRuntimeVersion()).specVersion.toNumber();
  console.log(`specVersion after: ${after}`);
  if (after !== 103) {
    console.error(`期望 specVersion 103，实际 ${after}`);
    process.exit(1);
  }
  console.log("OK — chain upgraded to spec 103. Run: npm run verify");
} finally {
  await api.disconnect();
}
