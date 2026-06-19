// EN: Node test WebSocket polyfill (ws package) for relay E2E.
// CN: Node 测试用 WebSocket 垫片（ws 包），供 relay E2E 使用。

import WS from "ws";

const g = globalThis as typeof globalThis & { WebSocket?: typeof WebSocket };
if (typeof g.WebSocket === "undefined") {
  g.WebSocket = WS as unknown as typeof WebSocket;
}
