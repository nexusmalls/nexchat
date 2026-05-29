#!/usr/bin/env tsx
/**
 * 列出指定 Entity 的所有会员，并标记本轮 pool reward 未领取会员。
 *
 * 用法:
 *   npx tsx list-unclaimed-pool-reward-members.ts --entity 100010 [--ws wss://rpc.nexusmall.net]
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
  claimableToken: bigint;
  alreadyClaimed: boolean;
  roundExpired: boolean;
  lastClaimedRound: bigint;
  effectiveLevel: number;
  isPaused: boolean;
}

interface MemberRow {
  address: string;
  effectiveLevel: number;
  currentRoundId: bigint;
  lastClaimedRound: bigint;
  alreadyClaimed: boolean;
  claimableNex: bigint;
  totalClaimedNex: bigint;
}

function printHelp(): void {
  console.log(`
列出指定 Entity 的所有会员，并标记本轮 pool reward 未领取会员

用法:
  npx tsx list-unclaimed-pool-reward-members.ts --entity 100010 [--ws wss://rpc.nexusmall.net]

必填参数:
  --entity, -e      实体 ID

可选参数:
  --ws              WebSocket 地址 (默认 wss://rpc.nexusmall.net)
  --out             导出 JSON 文件 (默认 reports/pool-reward-members-<entity>.json)
  --help, -h        显示帮助
`);
}

function parseArgs(): CliArgs {
  const argv = process.argv.slice(2);

  if (argv.includes('--help') || argv.includes('-h') || argv.length === 0) {
    printHelp();
    process.exit(0);
  }

  let entityId = NaN;
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

  if (isNaN(entityId) || entityId <= 0) {
    console.error('错误: 必须提供有效的 --entity 参数');
    process.exit(1);
  }

  if (!outFile) {
    outFile = `reports/pool-reward-members-${entityId}.json`;
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
    claimableToken: asBigInt(readObjectField(raw, 'claimableToken', 'claimable_token') ?? 0),
    alreadyClaimed: Boolean(readObjectField(raw, 'alreadyClaimed', 'already_claimed') ?? false),
    roundExpired: Boolean(readObjectField(raw, 'roundExpired', 'round_expired') ?? false),
    lastClaimedRound: asBigInt(readObjectField(raw, 'lastClaimedRound', 'last_claimed_round') ?? 0),
    effectiveLevel: coerceNumber(readObjectField(raw, 'effectiveLevel', 'effective_level')) ?? 0,
    isPaused: Boolean(readObjectField(raw, 'isPaused', 'is_paused') ?? false),
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

async function queryTotalClaimedNex(api: any, entityId: number, address: string): Promise<bigint> {
  try {
    const codec = await (api.query as any).commissionPoolReward.claimRecords(entityId, address);
    const records = codecToJson<any[]>(codec);
    if (!Array.isArray(records)) return 0n;
    return records.reduce((sum, record) => {
      const amount = asBigInt(readObjectField(record, 'amount') ?? 0);
      return sum + amount;
    }, 0n);
  } catch {
    return 0n;
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

  header('Entity 会员 pool reward 领取状态');
  kv('实体 ID', `${args.entityId}`);
  kv('节点', args.wsUrl);
  kv('导出文件', path.resolve(args.outFile));

  const api = await connectApi(args.wsUrl);

  try {
    const spec = `${api.runtimeVersion.specName} v${api.runtimeVersion.specVersion}`;
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();
    kv('链规格', spec);
    kv('当前区块', `#${currentBlock}`);

    subHeader('读取会员列表');
    const entries = await (api.query as any).entityMember.entityMembers.entries(args.entityId);
    const addresses: string[] = [];

    for (const [key] of entries) {
      const keyArgs = (key as any).args ?? [];
      const address = String(keyArgs[1] ?? '');
      if (address) addresses.push(address);
    }

    kv('会员总数', `${addresses.length}`);

    const rows: MemberRow[] = [];
    const skipped: string[] = [];
    let currentRoundId = 0n;

    for (const address of addresses) {
      const view = await queryMemberView(api, args.entityId, address);
      if (!view) {
        skipped.push(address);
        continue;
      }

      if (view.currentRoundId > currentRoundId) {
        currentRoundId = view.currentRoundId;
      }

      const totalClaimedNex = await queryTotalClaimedNex(api, args.entityId, address);
      rows.push({
        address,
        effectiveLevel: view.effectiveLevel,
        currentRoundId: view.currentRoundId,
        lastClaimedRound: view.lastClaimedRound,
        alreadyClaimed: view.alreadyClaimed,
        claimableNex: view.claimableNex,
        totalClaimedNex,
      });
    }

    const unclaimed = rows
      .filter((row) => !row.alreadyClaimed && row.claimableNex > 0n)
      .sort((a, b) => Number(b.claimableNex - a.claimableNex));

    const claimed = rows
      .filter((row) => row.alreadyClaimed)
      .sort((a, b) => Number(b.totalClaimedNex - a.totalClaimedNex));

    header('汇总');
    kv('当前轮次', `#${currentRoundId}`);
    kv('成功解析会员', `${rows.length}`);
    kv('跳过会员', `${skipped.length}`);
    kv('本轮未领取且可领', `${unclaimed.length}`);
    kv('本轮已领取', `${claimed.length}`);
    kv('未领取待发总额', formatNex(unclaimed.reduce((sum, row) => sum + row.claimableNex, 0n)));
    kv('历史已领取总额', formatNex(rows.reduce((sum, row) => sum + row.totalClaimedNex, 0n)));

    subHeader('未领取会员名单');
    if (unclaimed.length === 0) {
      console.log('  (无 / None)');
    } else {
      console.log(`  ${'地址'.padEnd(20)} ${'等级'.padStart(6)} ${'可领 NEX'.padStart(18)} ${'历史已领 NEX'.padStart(18)} ${'上次领取轮次'.padStart(12)}`);
      console.log(`  ${ln('─', 88)}`);
      for (const row of unclaimed) {
        console.log(
          `  ${shortAddr(row.address).padEnd(20)} ${`Lv${row.effectiveLevel}`.padStart(6)} ${formatNex(row.claimableNex).padStart(18)} ${formatNex(row.totalClaimedNex).padStart(18)} ${`#${row.lastClaimedRound}`.padStart(12)}`,
        );
      }
    }

    subHeader('全部会员领取明细');
    console.log(`  ${'地址'.padEnd(20)} ${'等级'.padStart(6)} ${'本轮状态'.padStart(10)} ${'可领 NEX'.padStart(18)} ${'历史已领 NEX'.padStart(18)} ${'上次领取轮次'.padStart(12)}`);
    console.log(`  ${ln('─', 100)}`);
    for (const row of rows) {
      const status = row.alreadyClaimed ? '已领取' : row.claimableNex > 0n ? '未领取' : '不可领';
      console.log(
        `  ${shortAddr(row.address).padEnd(20)} ${`Lv${row.effectiveLevel}`.padStart(6)} ${status.padStart(10)} ${formatNex(row.claimableNex).padStart(18)} ${formatNex(row.totalClaimedNex).padStart(18)} ${`#${row.lastClaimedRound}`.padStart(12)}`,
      );
    }

    if (skipped.length > 0) {
      subHeader('跳过的会员');
      for (const address of skipped) {
        console.log(`  ${address}`);
      }
    }

    await writeReport(args.outFile, {
      entityId: args.entityId,
      wsUrl: args.wsUrl,
      currentBlock,
      currentRoundId: currentRoundId.toString(),
      memberCount: addresses.length,
      parsedMemberCount: rows.length,
      skipped,
      summary: {
        unclaimedCount: unclaimed.length,
        claimedCount: claimed.length,
        unclaimedClaimableNex: unclaimed.reduce((sum, row) => sum + row.claimableNex, 0n).toString(),
        historicalClaimedNex: rows.reduce((sum, row) => sum + row.totalClaimedNex, 0n).toString(),
      },
      unclaimed: unclaimed.map((row) => ({
        address: row.address,
        effectiveLevel: row.effectiveLevel,
        currentRoundId: row.currentRoundId.toString(),
        lastClaimedRound: row.lastClaimedRound.toString(),
        claimableNex: row.claimableNex.toString(),
        totalClaimedNex: row.totalClaimedNex.toString(),
      })),
      members: rows.map((row) => ({
        address: row.address,
        effectiveLevel: row.effectiveLevel,
        currentRoundId: row.currentRoundId.toString(),
        lastClaimedRound: row.lastClaimedRound.toString(),
        alreadyClaimed: row.alreadyClaimed,
        claimableNex: row.claimableNex.toString(),
        totalClaimedNex: row.totalClaimedNex.toString(),
      })),
    });
    kv('结果已导出', path.resolve(args.outFile));

    console.log(`\n${ln('═')}`);
    console.log('  查询完成!');
    console.log(ln('═') + '\n');
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误:', err.message ?? err);
  process.exit(1);
});
