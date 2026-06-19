#!/usr/bin/env node
// EN: Run dual-end RFC 9474 delivery E2E (starts relay if needed).
// CN: 运行 RFC 9474 双端投递 E2E（必要时自动拉起 relay）。

import { execFileSync, spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import WS from "ws";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PORT = Number(process.env.RELAY_PORT ?? "8765");
const URL = process.env.VITE_RELAY_WS ?? `ws://127.0.0.1:${PORT}`;

async function up(url) {
  return new Promise((resolve) => {
    const ws = new WS(url);
    const t = setTimeout(() => {
      ws.close();
      resolve(false);
    }, 1500);
    ws.on("open", () => {
      clearTimeout(t);
      ws.close();
      resolve(true);
    });
    ws.on("error", () => {
      clearTimeout(t);
      resolve(false);
    });
  });
}

// EN: Build and spawn relay-rs for delivery E2E when the port is down.
// CN: 端口未监听时构建并拉起 relay-rs 以运行 delivery E2E。
const relayRsDir = path.join(root, "relay-rs");
let relay = null;
if (!(await up(URL))) {
  execFileSync("cargo", ["build", "--release", "-p", "relay-server"], {
    cwd: relayRsDir,
    stdio: "inherit",
  });
  relay = spawn(path.join(relayRsDir, "target/release/relay-server"), [], {
    cwd: root,
    env: { ...process.env, RELAY_PORT: String(PORT) },
    stdio: "inherit",
  });
  for (let i = 0; i < 25; i++) {
    if (await up(URL)) break;
    await new Promise((r) => setTimeout(r, 200));
  }
}

const vitest = spawn(
  "npx",
  ["vitest", "run", "src/delivery/delivery.e2e.test.ts"],
  {
    cwd: root,
    env: {
      ...process.env,
      DELIVERY_E2E: "1",
      VITE_RELAY_WS: URL,
      VITE_DELIVERY_TOKENS_ENABLED: "true",
    },
    stdio: "inherit",
  },
);

vitest.on("close", (code) => {
  relay?.kill("SIGTERM");
  process.exit(code ?? 1);
});
