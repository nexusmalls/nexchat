#!/usr/bin/env tsx
/**
 * 批量领取指定 Entity 的 pool reward。
 *
 * 用法:
 *   node --import tsx batch-claim-pool-reward.ts --entity 100010 --accounts file1.json --accounts file2.json
 *
 * 默认行为:
 *   - 读取提供的账户 JSON 文件并合并去重
 *   - 查询 entity 全部会员当前轮次状态
 *   - 仅对“本轮未领取且可领”的地址发起 claimPoolReward
 *   - 输出成功 / 失败 / 缺失账户清单，并导出 JSON 报告
 */

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { connectApi, disconnectApi, submitTx } from '../framework/api.js';
import { codecToJson, coerceNumber, readObjectField } from '../framework/codec.js';
import { asBigInt, formatNex } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

interface CliArgs {
  entityId: number;
  wsUrl: string;
  accountFiles: string[];
  outFile: string;
}

interface JsonAccountEntry {
  mnemonic: string;
  address: string;
  name?: string;
}

interface JsonAccountFile {
  accounts: JsonAccountEntry[];
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

interface MemberCandidate {
  address: string;
  mnemonic?: string;
  sourceFile?: string;
  effectiveLevel: number;
  claimableNex: bigint;
  currentRoundId: bigint;
  lastClaimedRound: bigint;
  alreadyClaimed: boolean;
}

function printHelp(): void {
  console.log(`
批量领取指定 Entity 的 pool reward

用法:
  node --import tsx batch-claim-pool-reward.ts --entity 100010 --accounts a.json --accounts b.json

必填参数:
  --entity, -e       实体 ID
  --accounts, -a     账户 JSON 文件，可重复传入多次

可选参数:
  --ws               WebSocket 地址 (默认 wss://rpc.nexusmall.net)
  --out              导出报告文件 (默认 reports/batch-claim-pool-reward-<entity>.json)
  --help, -h         显示帮助
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
  const accountFiles: string[] = [];

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case '--entity':
      case '-e':
        entityId = Number(argv[++i]);
        break;
      case '--accounts':
      case '-a':
        accountFiles.push(argv[++i] ?? '');
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
  if (accountFiles.length === 0) {
    console.error('错误: 至少提供一个 --accounts 文件');
    process.exit(1);
  }
  if (!outFile) {
    outFile = `reports/batch-claim-pool-reward-${entityId}.json`;
  }

  return { entityId, wsUrl, accountFiles, outFile };
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

async function loadAccounts(files: string[]): Promise<Map<string, { mnemonic: string; sourceFile: string }>> {
  const byAddress = new Map<string, { mnemonic: string; sourceFile: string }>();
  for (const file of files) {
    const abs = path.resolve(file);
    const raw = await readFile(abs, 'utf-8');
    const parsed = JSON.parse(raw) as JsonAccountFile;
    for (const account of parsed.accounts ?? []) {
      if (!account?.address || !account?.mnemonic) continue;
      if (!byAddress.has(account.address)) {
        byAddress.set(account.address, { mnemonic: account.mnemonic, sourceFile: abs });
      }
    }
  }
  return byAddress;
}

async function writeReport(filePath: string, payload: unknown): Promise<void> {
  const absPath = path.resolve(filePath);
  await mkdir(path.dirname(absPath), { recursive: true });
  await writeFile(absPath, JSON.stringify(payload, null, 2) + '\n', 'utf-8');
}

async function main(): Promise<void> {
  const args = parseArgs();
  process.env.WS_URL = args.wsUrl;

  header('批量领取 pool reward');
  kv('实体 ID', `${args.entityId}`);
  kv('节点', args.wsUrl);
  kv('账户文件数', `${args.accountFiles.length}`);
  kv('报告文件', path.resolve(args.outFile));

  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const localAccounts = await loadAccounts(args.accountFiles);
  kv('本地账户数(去重)', `${localAccounts.size}`);

  const api = await connectApi(args.wsUrl);

  try {
    const spec = `${api.runtimeVersion.specName} v${api.runtimeVersion.specVersion}`;
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();
    kv('链规格', spec);
    kv('当前区块', `#${currentBlock}`);

    subHeader('读取链上会员列表');
    const entries = await (api.query as any).entityMember.entityMembers.entries(args.entityId);
    const memberAddresses: string[] = [];
    for (const [key] of entries) {
      const keyArgs = (key as any).args ?? [];
      const address = String(keyArgs[1] ?? '');
      if (address) memberAddresses.push(address);
    }
    kv('链上会员总数', `${memberAddresses.length}`);

    const candidates: MemberCandidate[] = [];
    const skipped: string[] = [];
    let currentRoundId = 0n;
    let paused = false;
    let roundExpired = false;

    for (const address of memberAddresses) {
      const view = await queryMemberView(api, args.entityId, address);
      if (!view) {
        skipped.push(address);
        continue;
      }
      if (view.currentRoundId > currentRoundId) currentRoundId = view.currentRoundId;
      paused = paused || view.isPaused;
      roundExpired = roundExpired || view.roundExpired;
      const local = localAccounts.get(address);
      candidates.push({
        address,
        mnemonic: local?.mnemonic,
        sourceFile: local?.sourceFile,
        effectiveLevel: view.effectiveLevel,
        claimableNex: view.claimableNex,
        currentRoundId: view.currentRoundId,
        lastClaimedRound: view.lastClaimedRound,
        alreadyClaimed: view.alreadyClaimed,
      });
    }

    const claimable = candidates.filter((row) => !row.alreadyClaimed && row.claimableNex > 0n);
    const executable = claimable.filter((row) => !!row.mnemonic);
    const missingKey = claimable.filter((row) => !row.mnemonic);

    header('批量执行前汇总');
    kv('当前轮次', `#${currentRoundId}`);
    kv('会员视图成功数', `${candidates.length}`);
    kv('跳过账户', `${skipped.length}`);
    kv('本轮未领取且可领', `${claimable.length}`);
    kv('可直接执行', `${executable.length}`);
    kv('缺少助记词', `${missingKey.length}`);
    kv('奖池暂停', paused ? '是' : '否');
    kv('轮次过期', roundExpired ? '是' : '否');

    if (paused) {
      throw new Error('奖池当前已暂停，终止批量领取');
    }
    if (roundExpired) {
      throw new Error('当前轮次已过期，终止批量领取');
    }

    const success: Array<{ address: string; txHash: string; amount: string; roundId: string; levelId: number }> = [];
    const failed: Array<{ address: string; error: string }> = [];

    if (missingKey.length > 0) {
      subHeader('缺少本地助记词的可领会员');
      for (const row of missingKey) {
        console.log(`  ${shortAddr(row.address).padEnd(20)} ${`Lv${row.effectiveLevel}`.padStart(6)} ${formatNex(row.claimableNex).padStart(18)}`);
      }
    }

    subHeader('执行批量领取');
    for (const row of executable) {
      const signer = keyring.addFromMnemonic(row.mnemonic!);
      console.log(`  [执行] ${shortAddr(row.address)} Lv${row.effectiveLevel} ${formatNex(row.claimableNex)}`);

      const refreshed = await queryMemberView(api, args.entityId, row.address);
      if (!refreshed) {
        failed.push({ address: row.address, error: '无法重新获取会员视图' });
        console.log('    [失败] 无法重新获取会员视图');
        continue;
      }
      if (refreshed.alreadyClaimed) {
        console.log('    [跳过] 本轮已领取');
        continue;
      }
      if (refreshed.claimableNex === 0n) {
        console.log('    [跳过] 当前可领为 0');
        continue;
      }

      const extrinsic = (api.tx as any).commissionPoolReward.claimPoolReward(args.entityId);
      const receipt = await submitTx(api, extrinsic, signer, `batch claim ${row.address}`);
      if (!receipt.success) {
        failed.push({ address: row.address, error: receipt.error ?? '未知错误' });
        console.log(`    [失败] ${receipt.error ?? '未知错误'}`);
        continue;
      }

      const claimEvent = receipt.events.find(
        (e) => e.section === 'commissionPoolReward' && e.method === 'PoolRewardClaimed',
      );
      let amount = refreshed.claimableNex;
      let roundId = refreshed.currentRoundId;
      let levelId = refreshed.effectiveLevel;
      if (claimEvent) {
        const data = claimEvent.data as any;
        if (Array.isArray(data)) {
          amount = asBigInt(data[2]);
          roundId = asBigInt(data[4]);
          levelId = coerceNumber(data[5]) ?? levelId;
        } else {
          amount = asBigInt(readObjectField(data, 'amount') ?? amount);
          roundId = asBigInt(readObjectField(data, 'roundId', 'round_id') ?? roundId);
          levelId = coerceNumber(readObjectField(data, 'levelId', 'level_id')) ?? levelId;
        }
      }

      success.push({
        address: row.address,
        txHash: receipt.txHash,
        amount: amount.toString(),
        roundId: roundId.toString(),
        levelId,
      });
      console.log(`    [成功] ${receipt.txHash} ${formatNex(amount)}`);
    }

    header('批量领取结果');
    kv('成功笔数', `${success.length}`);
    kv('失败笔数', `${failed.length}`);
    kv('未覆盖可领账户', `${missingKey.length}`);
    kv('成功总额', formatNex(success.reduce((sum, row) => sum + BigInt(row.amount), 0n)));

    await writeReport(args.outFile, {
      entityId: args.entityId,
      wsUrl: args.wsUrl,
      currentBlock,
      currentRoundId: currentRoundId.toString(),
      accountFiles: args.accountFiles.map((file) => path.resolve(file)),
      localAccountCount: localAccounts.size,
      onchainMemberCount: memberAddresses.length,
      parsedMemberCount: candidates.length,
      skipped,
      summary: {
        claimableCount: claimable.length,
        executableCount: executable.length,
        missingKeyCount: missingKey.length,
        successCount: success.length,
        failedCount: failed.length,
        successTotalAmount: success.reduce((sum, row) => sum + BigInt(row.amount), 0n).toString(),
      },
      missingKey: missingKey.map((row) => ({
        address: row.address,
        effectiveLevel: row.effectiveLevel,
        claimableNex: row.claimableNex.toString(),
        currentRoundId: row.currentRoundId.toString(),
        lastClaimedRound: row.lastClaimedRound.toString(),
      })),
      success,
      failed,
    });
    kv('报告已导出', path.resolve(args.outFile));
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误:', err.message ?? err);
  process.exit(1);
});
