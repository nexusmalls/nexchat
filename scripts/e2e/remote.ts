#!/usr/bin/env tsx

const DEFAULT_REMOTE_WS_URL = 'wss://202.140.140.202';

process.env.WS_URL ??= DEFAULT_REMOTE_WS_URL;
process.env.NODE_TLS_REJECT_UNAUTHORIZED ??= '0';

console.log(`[远程 / remote] WS_URL=${process.env.WS_URL}`);
if (process.env.NODE_TLS_REJECT_UNAUTHORIZED === '0') {
  console.log('[远程 / remote] 已禁用 TLS 证书校验 / TLS certificate verification is disabled for this remote self-signed endpoint');
}

await import('./run.js');
