// EN: Shared relay-rs spawn helpers for vitest live e2e (delivery, Wire QA). Requires the release
// binary from `npm run relay:server` / `cargo build --release -p relay-server`.
// CN: vitest 在线 e2e（投递、Wire QA）共用的 relay-rs 拉起辅助。需 `npm run relay:server` 或
// `cargo build --release -p relay-server` 构建的 release 二进制。

import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import WS from "ws";

export function nexchatRoot(fromImportMetaUrl: string): string {
  return path.resolve(fileURLToPath(new URL("../..", fromImportMetaUrl)));
}

export function pickRelayBinary(root: string): { cmd: string; args: string[] } {
  const rustBin = path.join(root, "relay-rs/target/release/relay-server");
  if (!fs.existsSync(rustBin)) {
    throw new Error(
      "relay-rs binary not found — run: cd nexchat/relay-rs && cargo build --release -p relay-server",
    );
  }
  return { cmd: rustBin, args: [] };
}

export async function relayReachable(url: string, timeoutMs = 2000): Promise<boolean> {
  return new Promise((resolve) => {
    const ws = new WS(url);
    const t = setTimeout(() => {
      ws.close();
      resolve(false);
    }, timeoutMs);
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

export interface SpawnedRelay {
  proc: ChildProcess;
  dataDir: string | null;
  kill(): void;
}

/// EN: Start relay-rs on `port` (temp data dir optional). Waits until WS accepts connections.
/// CN: 在 `port` 启动 relay-rs（可选临时数据目录），直到 WS 可连接。
export async function spawnRelay(opts: {
  root: string;
  port: number;
  useTempDataDir?: boolean;
  waitAttempts?: number;
  waitMs?: number;
}): Promise<SpawnedRelay> {
  const dataDir = opts.useTempDataDir
    ? fs.mkdtempSync(path.join(os.tmpdir(), "nexchat-relay-e2e-"))
    : null;
  const { cmd, args } = pickRelayBinary(opts.root);
  const proc = spawn(cmd, args, {
    cwd: opts.root,
    env: {
      ...process.env,
      RELAY_PORT: String(opts.port),
      ...(dataDir ? { RELAY_DATA_DIR: dataDir } : {}),
    },
    stdio: "pipe",
  });
  const attempts = opts.waitAttempts ?? 50;
  const waitMs = opts.waitMs ?? 100;
  const url = process.env.VITE_RELAY_WS ?? `ws://127.0.0.1:${opts.port}`;
  for (let i = 0; i < attempts; i++) {
    if (await relayReachable(url)) break;
    await new Promise((r) => setTimeout(r, waitMs));
  }
  return {
    proc,
    dataDir,
    kill() {
      proc.kill("SIGTERM");
      if (dataDir) {
        try {
          fs.rmSync(dataDir, { recursive: true, force: true });
        } catch {
          /* ignore */
        }
      }
    },
  };
}

/// EN: Use an already-running relay or spawn relay-rs when the port is down.
/// CN: 复用已运行 relay；端口未监听时拉起 relay-rs。
export async function spawnRelayIfNeeded(opts: {
  root: string;
  port: number;
  url: string;
}): Promise<SpawnedRelay | null> {
  if (await relayReachable(opts.url)) return null;
  return spawnRelay({ root: opts.root, port: opts.port, waitAttempts: 30, waitMs: 200 });
}
