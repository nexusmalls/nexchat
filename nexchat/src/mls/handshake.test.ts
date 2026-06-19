// EN: Drives the MlsCoordinator handshake over an in-memory relay hub (simulating tabs):
// owner election → KeyPackage → add_members commit/welcome → epoch catch-up, then a real
// OpenMLS application message decrypts on every joined client. Deterministic stand-in for
// the flaky browser multi-tab test.
// CN: 用内存 relay hub（模拟标签页）驱动 MlsCoordinator 握手：owner 选举→KeyPackage→add_members
// commit/welcome→epoch 补齐，随后真实 OpenMLS 应用消息在每个已入群客户端解密成功。替代易抖动的
// 浏览器多标签页测试。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { MlsCoordinator, type MlsStatus } from "@/mls/handshake";
import { textEnvelope } from "@/mls/envelope";
import type {
  ControlInbound,
  ControlMsg,
  RelayClient,
  RelayFrame,
  RelayInbound,
} from "@/relay/relayClient";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// EN: in-memory broadcast hub; each client delivers to every OTHER client. CN: 内存广播 hub。
class TestHub {
  private clients: { id: string; ctrl?: ControlInbound; msg?: RelayInbound }[] = [];
  client(id: string): RelayClient {
    const entry: { id: string; ctrl?: ControlInbound; msg?: RelayInbound } = { id };
    this.clients.push(entry);
    return {
      connect: async () => {},
      disconnect: () => {},
      send: async (frame: RelayFrame) => {
        for (const c of this.clients) if (c.id !== id) c.msg?.(frame);
      },
      sendControl: async (m: ControlMsg) => {
        for (const c of this.clients) if (c.id !== id) c.ctrl?.(m);
      },
      onMessage: (cb: RelayInbound) => {
        entry.msg = cb;
      },
      onControl: (cb: ControlInbound) => {
        entry.ctrl = cb;
      },
    };
  }
}

async function makeTab(hub: TestHub, id: string, identity: string) {
  const engine = new OpenMlsEngine();
  await engine.init(identity);
  let status: MlsStatus = { role: "unknown", ready: false, members: 0 };
  const coord = new MlsCoordinator({
    engine,
    relay: hub.client(id),
    endpointId: id,
    identity,
    groupId: 0,
    onStatus: (s) => (status = s),
  });
  return { engine, coord, getStatus: () => status };
}

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

describe("MlsCoordinator handshake over relay", () => {
  it("two tabs: owner election → group → real MLS message round-trip", async () => {
    const hub = new TestHub();
    const a = await makeTab(hub, "A", "alice");
    const b = await makeTab(hub, "B", "bob");

    a.coord.start();
    b.coord.start();
    await sleep(900); // let settle + add_members + welcome complete

    expect(a.engine.hasGroup("g:0")).toBe(true);
    expect(b.engine.hasGroup("g:0")).toBe(true);
    expect(a.getStatus().role).toBe("owner"); // "A" < "B"
    expect(b.getStatus().role).toBe("member");
    expect(b.getStatus().ready).toBe(true);

    // real OpenMLS application message: owner → member
    const ct = await a.engine.encrypt("g:0", textEnvelope("m1", "hi from owner", {}));
    const env = await b.engine.decrypt("g:0", ct);
    expect((env.body as { text: string }).text).toBe("hi from owner");

    // member → owner
    const ct2 = await b.engine.encrypt("g:0", textEnvelope("m2", "reply from member", {}));
    const env2 = await a.engine.decrypt("g:0", ct2);
    expect((env2.body as { text: string }).text).toBe("reply from member");
  });

  it("third tab joins later: epoch catch-up keeps all members in sync", async () => {
    const hub = new TestHub();
    const a = await makeTab(hub, "A", "alice");
    const b = await makeTab(hub, "B", "bob");
    a.coord.start();
    b.coord.start();
    await sleep(900);

    // C joins after the group already exists → owner commits (B catches up via Commit)
    const c = await makeTab(hub, "C", "charlie");
    c.coord.start();
    await sleep(900);

    expect(c.engine.hasGroup("g:0")).toBe(true);
    expect(c.getStatus().role).toBe("member");

    // owner sends at the latest epoch; BOTH B (caught up via Commit) and C (via Welcome) read it
    const ct = await a.engine.encrypt("g:0", textEnvelope("m3", "all three", {}));
    expect((((await b.engine.decrypt("g:0", ct)).body) as { text: string }).text).toBe("all three");
    expect((((await c.engine.decrypt("g:0", ct)).body) as { text: string }).text).toBe("all three");
  });
});
