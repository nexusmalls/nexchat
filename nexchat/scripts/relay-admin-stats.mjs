#!/usr/bin/env node
// EN: Query relay admin_stats (chat mailbox metrics) over WebSocket.
// CN: 经 WebSocket 查询 relay admin_stats（含聊天邮箱指标）。

import WebSocket from "ws";

const relay = process.env.RELAY_WS ?? `ws://127.0.0.1:${process.env.RELAY_PORT ?? "8765"}`;
const secret = process.env.RELAY_ADMIN_SECRET ?? "";
const requestId = process.argv[2] ?? `stats-${Date.now()}`;

const ws = new WebSocket(relay);
const t = setTimeout(() => {
  console.error("timeout");
  ws.close();
  process.exit(1);
}, 8000);

ws.on("open", () => {
  ws.send(
    JSON.stringify({
      type: "admin_stats",
      request_id: requestId,
      ...(secret ? { admin_secret: secret } : {}),
    }),
  );
});

ws.on("message", (raw) => {
  try {
    const m = JSON.parse(String(raw));
    if (m.type !== "admin_stats_reply" || m.request_id !== requestId) return;
    clearTimeout(t);
    console.log(JSON.stringify(m.stats, null, 2));
    ws.close();
    process.exit(0);
  } catch (e) {
    clearTimeout(t);
    console.error(e);
    ws.close();
    process.exit(1);
  }
});

ws.on("error", (e) => {
  clearTimeout(t);
  console.error(e.message);
  process.exit(1);
});
