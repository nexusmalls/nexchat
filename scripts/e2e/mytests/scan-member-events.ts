#!/usr/bin/env tsx

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';

const ACCOUNT = process.argv[2] ?? 'X4TjpDzhtqvxzzt49Fop2653xuV8AS8SXcebzK7JVpKGYa8y1';
const FROM_BLOCK = Number(process.argv[3] ?? '12000');
const TO_BLOCK = Number(process.argv[4] ?? '40050');

function eventSummary(ev: any) {
  return {
    section: ev.event.section,
    method: ev.event.method,
    data: codecToJson(ev.event.data),
    phase: ev.phase?.toString?.() ?? String(ev.phase),
  };
}

function containsAccount(value: unknown, account: string): boolean {
  if (value == null) return false;
  if (typeof value === 'string') return value === account;
  if (Array.isArray(value)) return value.some((v) => containsAccount(v, account));
  if (typeof value === 'object') return Object.values(value as Record<string, unknown>).some((v) => containsAccount(v, account));
  return false;
}

async function main(): Promise<void> {
  const api = await connectApi();
  try {
    const rows: Array<Record<string, unknown>> = [];

    for (let block = FROM_BLOCK; block <= TO_BLOCK; block++) {
      const hash = await api.rpc.chain.getBlockHash(block);
      const events = await api.query.system.events.at(hash);
      const arr = Array.from(events as unknown as any[]);

      for (const ev of arr) {
        const section = ev.event.section;
        const method = ev.event.method;
        if (section !== 'entityMember' && section !== 'entityTransaction') continue;

        const data = codecToJson(ev.event.data);
        if (!containsAccount(data, ACCOUNT)) continue;

        rows.push({
          block,
          section,
          method,
          data,
          summary: eventSummary(ev),
        });
      }
    }

    console.log(JSON.stringify({ account: ACCOUNT, fromBlock: FROM_BLOCK, toBlock: TO_BLOCK, count: rows.length, rows }, null, 2));
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
