#!/usr/bin/env tsx
/**
 * 导出指定 Entity 下 Lv1 会员当前的 pool reward 领取明细。
 *
 * 输出字段：
 * - address
 * - claimableNex
 * - alreadyClaimed
 * - lastClaimedRound
 */

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, coerceNumber, readObjectField } from '../framework/codec.js';
import { asBigInt, formatNex } from '../framework/units.js';

interface CliArgs {
  entityId: number;
  wsUrl: string;
  outFile: string;
}

interface PoolRewardMemberView {
  currentRoundId: bigint;
  claimableNex: bigint;
  alreadyClaimed: boolean;
  lastClaimedRound: bigint;
  effectiveLevel: number;
}

interface Lv1Row {
  address: string;
  claimableNex: bigint;
  alreadyClaimed: boolean;
  lastClaimedRound: bigint;
  currentRoundId: bigint;
}

function printHelp(): void {
  console.log(`
导出指定 Entity 下 Lv1 会员当前的 pool reward 领取明细

用法:
  node --import tsx export-lv1-pool-reward-status.ts --entity 100010

可选参数:
  --entity, -e   实体 ID (默认 100010)
  --ws           WebSocket 地址 (默认 wss://rpc.nexusmall.net)
  --out          导出 JSON 文件 (默认 reports/lv1-pool-reward-status-<entity>.json)
  --help, -h     显示帮助
`);
}

function parseArgs(): CliArgs {
  const argv = process.argv.slice(2);
  if (argv.includes('--help') || argv.includes('-h')) {
    printHelp();
    process.exit(0);
  }

  let entityId = 100010;
  let wsUrl = 'wss://rpc.nexusmall.net';
  let outFile = '';

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case '--entity':
      case '-e':
        entityId = Number(argv[++i]);
        break;
      case '--ws':
        wsUrl = argv[++i] ?? wsUrl;
        break;
      case '--out':
        outFile = argv[++i] ?? outFile;
        break;
      default:
        console.error(`未知参数: ${arg}`);
        printHelp();
        process.exit(1);
    }
  }

  if (!Number.isInteger(entityId) || entityId <= 0) {
    console.error('错误: 必须提供有效的 --entity 参数');
    process.exit(1);
  }

  if (!outFile) {
    outFile = `reports/lv1-pool-reward-status-${entityId}.json`;
  }

  return { entityId, wsUrl, outFile };
}

function ln(char = '─', len = 100): string {
  return char.repeat(len);
}

function header(title: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${title}`);
  console.log(ln('═'));
}

function subHeader(title: string): void {
  console.log(`\n${ln('─')}`);
  console.log(`  ${title}`);
  console.log(ln('─'));
}

function kv(label: string, value: string): void {
  console.log(`  ${label.padEnd(24)} ${value}`);
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

function parseMemberView(raw: Record<string, unknown>): PoolRewardMemberView {
  return {
    currentRoundId: asBigInt(readObjectField(raw, 'currentRoundId', 'current_round_id') ?? 0),
    claimableNex: asBigInt(readObjectField(raw, 'claimableNex', 'claimable_nex') ?? 0),
    alreadyClaimed: Boolean(readObjectField(raw, 'alreadyClaimed', 'already_claimed') ?? false),
    lastClaimedRound: asBigInt(readObjectField(raw, 'lastClaimedRound', 'last_claimed_round') ?? 0),
    effectiveLevel: coerceNumber(readObjectField(raw, 'effectiveLevel', 'effective_level')) ?? 0,
  };
}

async function queryMemberView(api: any, entityId: number, address: string): Promise<PoolRewardMemberView | null> {
  try {
    const codec = await api.call.poolRewardDetailApi?.getPoolRewardMemberView?.(entityId, address);
    if (!codec) return null;
    const parsed = codecToJson<Record<string, unknown> | null>(codec);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null;
    return parseMemberView(parsed);
  } catch {
    return null;
  }
}

async function writeReport(filePath: string, payload: unknown): Promise<void> {
  const absPath = path.resolve(filePath);
  await mkdir(path.dirname(absPath), { recursive: true });
  await writeFile(absPath, JSON.stringify(payload, null, 2) + '\n', 'utf-8');
}

async function main(): Promise<void> {
  const args = parseArgs();
  process.env.WS_URL = args.wsUrl;

  header('导出 Lv1 pool reward 明细');
  kv('实体 ID', `${args.entityId}`);
  kv('节点', args.wsUrl);
  kv('导出文件', path.resolve(args.outFile));

  const api = await connectApi(args.wsUrl);

  try {
    const spec = `${api.runtimeVersion.specName} v${api.runtimeVersion.specVersion}`;
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();
    kv('链规格', spec);
    kv('当前区块', `#${currentBlock}`);

    subHeader('读取会员列表并筛选 Lv1');
    const entries = await (api.query as any).entityMember.entityMembers.entries(args.entityId);
    const memberAddresses: string[] = [];
    for (const [key] of entries) {
      const keyArgs = (key as any).args ?? [];
      const address = String(keyArgs[1] ?? '');
      if (address) memberAddresses.push(address);
    }
    kv('链上会员总数', `${memberAddresses.length}`);

    const rows: Lv1Row[] = [];
    const skipped: string[] = [];
    let currentRoundId = 0n;

    for (const address of memberAddresses) {
      const view = await queryMemberView(api, args.entityId, address);
      if (!view) {
        skipped.push(address);
        continue;
      }
      if (view.currentRoundId > currentRoundId) currentRoundId = view.currentRoundId;
      if (view.effectiveLevel !== 1) continue;
      rows.push({
        address,
        claimableNex: view.claimableNex,
        alreadyClaimed: view.alreadyClaimed,
        lastClaimedRound: view.lastClaimedRound,
        currentRoundId: view.currentRoundId,
      });
    }

    rows.sort((a, b) => {
      if (a.claimableNex !== b.claimableNex) return a.claimableNex > b.claimableNex ? -1 : 1;
      return a.address.localeCompare(b.address);
    });

    header('Lv1 汇总');
    kv('当前轮次', `#${currentRoundId}`);
    kv('Lv1 会员数', `${rows.length}`);
    kv('可领人数', `${rows.filter((row) => row.claimableNex > 0n).length}`);
    kv('已领取标记数', `${rows.filter((row) => row.alreadyClaimed).length}`);
    kv('待领取总额', formatNex(rows.reduce((sum, row) => sum + row.claimableNex, 0n)));
    kv('跳过账户', `${skipped.length}`);

    subHeader('Lv1 会员明细');
    console.log(`  ${'地址'.padEnd(20)} ${'claimableNex'.padStart(18)} ${'alreadyClaimed'.padStart(16)} ${'lastClaimedRound'.padStart(18)}`);
    console.log(`  ${ln('─', 80)}`);
    for (const row of rows) {
      console.log(
        `  ${shortAddr(row.address).padEnd(20)} ${formatNex(row.claimableNex).padStart(18)} ${String(row.alreadyClaimed).padStart(16)} ${`#${row.lastClaimedRound}`.padStart(18)}`,
      );
    }

    await writeReport(args.outFile, {
      entityId: args.entityId,
      wsUrl: args.wsUrl,
      currentBlock,
      currentRoundId: currentRoundId.toString(),
      memberCount: memberAddresses.length,
      lv1Count: rows.length,
      skipped,
      summary: {
        claimableCount: rows.filter((row) => row.claimableNex > 0n).length,
        alreadyClaimedCount: rows.filter((row) => row.alreadyClaimed).length,
        totalClaimableNex: rows.reduce((sum, row) => sum + row.claimableNex, 0n).toString(),
      },
      rows: rows.map((row) => ({
        address: row.address,
        claimableNex: row.claimableNex.toString(),
        alreadyClaimed: row.alreadyClaimed,
        lastClaimedRound: row.lastClaimedRound.toString(),
        currentRoundId: row.currentRoundId.toString(),
      })),
    });

    kv('结果已导出', path.resolve(args.outFile));
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误:', err.message ?? err);
  process.exit(1);
});
