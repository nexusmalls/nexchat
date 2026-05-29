#!/usr/bin/env tsx
/**
 * 从 JSON 文件批量逐笔转账 NEX
 *
 * 用法:
 *   node --import tsx e2e/mytests/bulk-transfer-from-json.ts --file ./transfers.json --amount 10
 *   node --import tsx e2e/mytests/bulk-transfer-from-json.ts --file ./transfers.json --from bob --dry-run
 *   node --import tsx e2e/mytests/bulk-transfer-from-json.ts --file ./accounts.json --random-pairs --count 2000
 *
 * 支持的 JSON 结构:
 *   1. 对象映射
 *      {
 *        "5F...": "10",
 *        "5G...": { "amount": "0.5", "name": "user-2" }
 *      }
 *
 *   2. 包裹数组
 *      {
 *        "accounts": [
 *          { "address": "5F...", "amount": "10", "name": "user-1" },
 *          { "address": "5G..." }
 *        ]
 *      }
 *
 *   3. 纯数组
 *      [
 *        { "address": "5F...", "amount": "10" },
 *        { "address": "5G..." }
 *      ]
 *
 * 规则:
 *   - 每条记录可单独指定 amount (单位 NEX)
 *   - 未指定 amount 时：若传了 --amount 则使用该默认值，否则为该记录随机生成 600000 - 1200000 NEX
 *   - 逐笔提交 transferKeepAlive，任一失败即停止
 *   - 启用 --random-pairs 后：从 JSON 账户中随机选付款方和收款方（不能相同）
 *   - random-pairs 模式下每笔金额随机为 100000 - 500000 NEX，并强制每个账户剩余余额大于 500000 NEX
 *   - random-pairs 模式以成功笔数作为停止条件；--count 2000 表示成功笔数超过 2000（即 2001 笔）才停止
 */

import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { connectApi, disconnectApi, submitTx } from '../framework/api.js';
import { getDevActors, getSelectedActorsFilePath, readFreeBalance } from '../framework/accounts.js';
import { formatNex, NEX_PLANCK } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

interface CliArgs {
  filePaths: string[];
  from: string;
  fromMnemonic: string | null;
  defaultAmount: bigint | null;
  useRandomDefaultAmount: boolean;
  wsUrl: string;
  dryRun: boolean;
  start: number;
  limit: number | null;
  randomPairs: boolean;
  randomCount: number;
}

type JsonAmountValue = string | number | { amount?: string | number; name?: string } | null;

const DEFAULT_TRANSFER_NEX = 5_001_000;
const MIN_ACCOUNT_BALANCE_NEX = 500_000;
const TARGET_ACCOUNT_BALANCE_NEX = 600_000;
const RANDOM_TRANSFER_MIN_NEX = 100_000;
const RANDOM_TRANSFER_MAX_NEX = 500_000;
const DEFAULT_JSON_FILES = [
  'e2e/mytests/test-accounts-2026-04-15T05-59-05-187Z100010.json',
  'e2e/mytests/test-accounts-2026-04-15T05-57-40-385Z100010.json',
  'e2e/mytests/test-accounts-2026-04-15T05-57-36-684Z100010.json',
  'e2e/mytests/test-accounts-2026-04-15T05-57-31-426Z100010.json',
  'e2e/mytests/test-accounts-2026-04-15T05-56-58-825Z100010.json',
] as const;

interface JsonArrayRecord {
  address?: string;
  amount?: string | number;
  name?: string;
  mnemonic?: string;
}

interface RandomPairTransfer {
  originalIndex: number;
  fromAddress: string;
  fromName?: string;
  fromMnemonic: string;
  toAddress: string;
  toName?: string;
  amount: bigint;
}

interface TransferRecord {
  originalIndex: number;
  address: string;
  name?: string;
  amount: bigint;
  mnemonic?: string;
}

function printHelp(): void {
  console.log(`
从 JSON 文件批量逐笔转账 NEX

用法:
  node --import tsx e2e/mytests/bulk-transfer-from-json.ts --file ./transfers.json [选项]

可选参数:
  --file,   -f    JSON 文件路径；可重复传入，或用逗号分隔多个文件；不传则使用脚本内置 5 个账户文件
  --amount, -a    默认转账金额 (NEX)，普通模式下当记录未设置 amount 时使用；random-pairs 模式忽略此参数
  --from          付款账户角色名，默认 alice
  --from-mnemonic 指定付款账户助记词；传入后优先于 --from
  --ws            WebSocket 地址 (默认 wss://rpc.nexusmall.net)
  --start         从第几条开始处理 (默认 0)
  --limit         最多处理多少条
  --random-pairs  从 JSON 中随机选付款方和收款方逐笔互转
  --count         random-pairs 模式下成功笔数阈值；2000 表示成功超过 2000 笔才停止
  --dry-run       仅打印计划，不提交交易
  --help,   -h    显示帮助

示例:
  node --import tsx e2e/mytests/bulk-transfer-from-json.ts \
    --file ./e2e/mytests/transfers.json \
    --amount 10

  node --import tsx e2e/mytests/bulk-transfer-from-json.ts \
    --file ./e2e/mytests/transfers.json \
    --from bob \
    --start 100 \
    --limit 20 \
    --dry-run

  node --import tsx e2e/mytests/bulk-transfer-from-json.ts \
    --file ./e2e/mytests/accounts.json \
    --random-pairs \
    --count 2000
`);
}

function parseArgs(): CliArgs {
  const argv = process.argv.slice(2);

  if (argv.includes('--help') || argv.includes('-h')) {
    printHelp();
    process.exit(0);
  }

  let filePaths: string[] = [];
  let from = 'alice';
  let fromMnemonic: string | null = 'gasp banana raw crunch mother sunset assault chicken dust blue dust universe';
  let defaultAmount: bigint | null = null;
  let useRandomDefaultAmount = true;
  let wsUrl = 'wss://rpc.nexusmall.net';
  let dryRun = false;
  let start = 0;
  let limit: number | null = null;
  let randomPairs = false;
  let randomCount = 2000;

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case '--file':
      case '-f':
        filePaths.push(...(argv[++i] ?? '').split(',').map((value) => value.trim()).filter(Boolean));
        break;
      case '--amount':
      case '-a':
        defaultAmount = parseNexToPlanck(argv[++i] ?? '', '--amount');
        useRandomDefaultAmount = false;
        break;
      case '--from':
        from = (argv[++i] ?? '').trim() || from;
        break;
      case '--from-mnemonic':
        fromMnemonic = (argv[++i] ?? '').trim() || null;
        break;
      case '--ws':
        wsUrl = argv[++i] ?? wsUrl;
        break;
      case '--dry-run':
        dryRun = true;
        break;
      case '--random-pairs':
        randomPairs = true;
        break;
      case '--count': {
        const value = Number(argv[++i]);
        if (!Number.isInteger(value) || value <= 0) {
          console.error('错误: --count 必须是正整数');
          process.exit(1);
        }
        randomCount = value;
        break;
      }
      case '--start': {
        const value = Number(argv[++i]);
        if (!Number.isInteger(value) || value < 0) {
          console.error('错误: --start 必须是大于等于 0 的整数');
          process.exit(1);
        }
        start = value;
        break;
      }
      case '--limit': {
        const value = Number(argv[++i]);
        if (!Number.isInteger(value) || value <= 0) {
          console.error('错误: --limit 必须是正整数');
          process.exit(1);
        }
        limit = value;
        break;
      }
      default:
        console.error(`未知参数: ${arg}`);
        printHelp();
        process.exit(1);
    }
  }

  if (filePaths.length === 0) {
    filePaths = [...DEFAULT_JSON_FILES];
  }

  return { filePaths, from, fromMnemonic, defaultAmount, useRandomDefaultAmount, wsUrl, dryRun, start, limit, randomPairs, randomCount };
}

function ln(char = '─', len = 76): string {
  return char.repeat(len);
}

function header(title: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${title}`);
  console.log(ln('═'));
}

function kv(label: string, value: string): void {
  console.log(`  ${label.padEnd(20)} ${value}`);
}

function shortAddr(addr: string): string {
  if (addr.length <= 18) {
    return addr;
  }
  return `${addr.slice(0, 8)}...${addr.slice(-8)}`;
}

function parseNexToPlanck(input: string | number, field: string): bigint {
  const raw = typeof input === 'number' ? String(input) : input.trim();
  const cleaned = raw.replace(/,/g, '');

  if (!cleaned) {
    throw new Error(`${field} 不能为空`);
  }

  if (!/^\d+(\.\d+)?$/.test(cleaned)) {
    throw new Error(`${field} 必须是非负数字，当前值: ${raw}`);
  }

  const [wholePart, fractionPart = ''] = cleaned.split('.');
  if (fractionPart.length > 12) {
    throw new Error(`${field} 最多支持 12 位小数，当前值: ${raw}`);
  }

  const whole = BigInt(wholePart) * NEX_PLANCK;
  const paddedFraction = fractionPart.padEnd(12, '0');
  const fraction = paddedFraction ? BigInt(paddedFraction) : 0n;
  const total = whole + fraction;

  if (total <= 0n) {
    throw new Error(`${field} 必须大于 0，当前值: ${raw}`);
  }

  return total;
}

function resolveDefaultAmount(defaultAmount: bigint | null, useRandomDefaultAmount: boolean): bigint | null {
  if (defaultAmount != null) {
    return defaultAmount;
  }
  if (useRandomDefaultAmount) {
    return BigInt(DEFAULT_TRANSFER_NEX) * NEX_PLANCK;
  }
  return null;
}

function randomIntInclusive(min: number, max: number): number {
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

function randomTransferAmount(): bigint {
  return BigInt(randomIntInclusive(RANDOM_TRANSFER_MIN_NEX, RANDOM_TRANSFER_MAX_NEX)) * NEX_PLANCK;
}

function minimumAccountBalance(): bigint {
  return BigInt(MIN_ACCOUNT_BALANCE_NEX) * NEX_PLANCK;
}

function targetAccountBalance(): bigint {
  return BigInt(TARGET_ACCOUNT_BALANCE_NEX) * NEX_PLANCK;
}

function countBelowTargetBalances(balances: Map<string, bigint>): number {
  const target = targetAccountBalance();
  let count = 0;
  for (const balance of balances.values()) {
    if (balance < target) {
      count += 1;
    }
  }
  return count;
}

function totalShortfall(balances: Map<string, bigint>): bigint {
  const target = targetAccountBalance();
  let total = 0n;
  for (const balance of balances.values()) {
    if (balance < target) {
      total += target - balance;
    }
  }
  return total;
}

function totalSpendable(records: TransferRecord[], balances: Map<string, bigint>): bigint {
  const minimum = minimumAccountBalance();
  let total = 0n;
  for (const record of records) {
    const balance = balances.get(record.address) ?? 0n;
    if (balance > minimum) {
      total += balance - minimum;
    }
  }
  return total;
}

function normalizeObjectMapping(
  parsed: Record<string, JsonAmountValue>,
  defaultAmount: bigint | null,
): TransferRecord[] {
  const records: TransferRecord[] = [];
  let index = 0;

  for (const [address, value] of Object.entries(parsed)) {
    if (!address.trim()) {
      throw new Error(`第 ${index} 条记录地址为空`);
    }

    let name: string | undefined;
    let amountRaw: string | number | undefined;

    if (typeof value === 'string' || typeof value === 'number') {
      amountRaw = value;
    } else if (value && typeof value === 'object') {
      amountRaw = value.amount;
      name = value.name?.trim() || undefined;
    }

    const amount = amountRaw != null
      ? parseNexToPlanck(amountRaw, `address=${address} 的 amount`)
      : resolveDefaultAmount(defaultAmount, true);

    if (amount == null) {
      throw new Error(`地址 ${address} 未设置 amount，且未提供 --amount 默认值`);
    }

    records.push({
      originalIndex: index,
      address: address.trim(),
      name,
      amount,
    });
    index += 1;
  }

  return records;
}

function normalizeArrayRecords(records: JsonArrayRecord[], defaultAmount: bigint | null): TransferRecord[] {
  return records.map((record, index) => {
    const address = record.address?.trim() ?? '';
    if (!address) {
      throw new Error(`第 ${index} 条记录缺少有效 address`);
    }

    const amount = record.amount != null
      ? parseNexToPlanck(record.amount, `第 ${index} 条记录 amount`)
      : resolveDefaultAmount(defaultAmount, true);

    if (amount == null) {
      throw new Error(`第 ${index} 条记录未设置 amount，且未提供 --amount 默认值`);
    }

    return {
      originalIndex: index,
      address,
      name: record.name?.trim() || undefined,
      amount,
      mnemonic: record.mnemonic?.trim() || undefined,
    };
  });
}

function normalizeTransfers(raw: unknown, defaultAmount: bigint | null): TransferRecord[] {
  if (Array.isArray(raw)) {
    return normalizeArrayRecords(raw as JsonArrayRecord[], defaultAmount);
  }

  if (raw && typeof raw === 'object') {
    const maybeAccounts = (raw as { accounts?: unknown }).accounts;
    if (Array.isArray(maybeAccounts)) {
      return normalizeArrayRecords(maybeAccounts as JsonArrayRecord[], defaultAmount);
    }
    return normalizeObjectMapping(raw as Record<string, JsonAmountValue>, defaultAmount);
  }

  throw new Error('JSON 顶层必须是对象映射、{ accounts: [...] } 或数组');
}

async function loadTransfers(filePath: string, defaultAmount: bigint | null): Promise<TransferRecord[]> {
  const raw = await readFile(filePath, 'utf-8');
  const parsed = JSON.parse(raw) as unknown;
  const records = normalizeTransfers(parsed, defaultAmount);

  if (records.length === 0) {
    throw new Error('JSON 中没有可处理的转账记录');
  }

  return records;
}

async function loadTransfersFromFiles(filePaths: string[], defaultAmount: bigint | null): Promise<TransferRecord[]> {
  const records: TransferRecord[] = [];
  for (const filePath of filePaths) {
    const fileRecords = await loadTransfers(filePath, defaultAmount);
    records.push(...fileRecords);
  }
  return records;
}

function createSignerFromMnemonic(mnemonic: string) {
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  return keyring.addFromMnemonic(mnemonic);
}

function pickRandomIndex(max: number): number {
  return Math.floor(Math.random() * max);
}

function pickRandomSenderCandidates(records: TransferRecord[]): TransferRecord[] {
  const shuffled = [...records];
  for (let i = shuffled.length - 1; i > 0; i--) {
    const j = pickRandomIndex(i + 1);
    [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
  }
  return shuffled;
}

function pickPreferredRecipient(records: TransferRecord[], balances: Map<string, bigint>, excludeAddress: string): TransferRecord | null {
  const target = targetAccountBalance();
  const belowTarget = records.filter((record) => record.address !== excludeAddress && (balances.get(record.address) ?? 0n) < target);
  if (belowTarget.length > 0) {
    return belowTarget[pickRandomIndex(belowTarget.length)];
  }

  const candidates = records.filter((record) => record.address !== excludeAddress);
  if (candidates.length === 0) {
    return null;
  }
  return candidates[pickRandomIndex(candidates.length)];
}

function pickRandomTransferAmount(maxAllowed: bigint): bigint | null {
  const minTransfer = BigInt(RANDOM_TRANSFER_MIN_NEX) * NEX_PLANCK;
  const maxTransferCap = BigInt(RANDOM_TRANSFER_MAX_NEX) * NEX_PLANCK;
  const upperBound = maxAllowed < maxTransferCap ? maxAllowed : maxTransferCap;
  const bannedAmount = BigInt(RANDOM_TRANSFER_MIN_NEX) * NEX_PLANCK;
  if (upperBound <= bannedAmount) {
    return null;
  }

  const minAllowed = RANDOM_TRANSFER_MIN_NEX + 1;
  const upperNex = Number(upperBound / NEX_PLANCK);
  return BigInt(randomIntInclusive(minAllowed, upperNex)) * NEX_PLANCK;
}

async function loadSigner(args: CliArgs) {
  if (args.fromMnemonic) {
    await cryptoWaitReady();
    return {
      signer: createSignerFromMnemonic(args.fromMnemonic),
      source: 'mnemonic',
      actorsFile: undefined as string | undefined,
    };
  }

  const actors = await getDevActors();
  const signer = actors[args.from];
  if (!signer) {
    throw new Error(`未找到付款账户角色 ${args.from}`);
  }

  return {
    signer,
    source: `dev actor: ${args.from}`,
    actorsFile: getSelectedActorsFilePath(),
  };
}

function formatAccountLabel(address: string, name?: string): string {
  return name ? `${name} (${shortAddr(address)})` : shortAddr(address);
}

async function loadInitialBalances(api: Awaited<ReturnType<typeof connectApi>>, records: TransferRecord[]): Promise<Map<string, bigint>> {
  const balances = new Map<string, bigint>();
  for (const record of records) {
    balances.set(record.address, await readFreeBalance(api, record.address));
  }
  return balances;
}

async function main(): Promise<void> {
  const args = parseArgs();
  const resolvedFilePaths = args.filePaths.map((filePath) => path.resolve(filePath));
  const allRecords = await loadTransfersFromFiles(resolvedFilePaths, args.defaultAmount);
  const baseRecords = args.limit == null
    ? allRecords.slice(args.start)
    : allRecords.slice(args.start, args.start + args.limit);
  const selectedRecords = args.randomPairs ? [] : baseRecords;

  let api: Awaited<ReturnType<typeof connectApi>> | undefined;
  let totalAmount = selectedRecords.reduce((sum, record) => sum + record.amount, 0n);

  try {
    process.env.WS_URL = args.wsUrl;
    api = await connectApi(args.wsUrl);

    if (args.randomPairs) {
      await loadInitialBalances(api, baseRecords);
    }

    header(args.randomPairs ? 'JSON 随机互转' : 'JSON 批量转账');
    kv('文件', `${resolvedFilePaths.length} 个`);
    for (const filePath of resolvedFilePaths) {
      console.log(`  ${''.padEnd(20)} ${filePath}`);
    }
    kv('付款账户', args.randomPairs ? '随机账户池' : (args.fromMnemonic ? 'from-mnemonic' : args.from));
    kv('节点', args.wsUrl);
    kv('默认金额', args.randomPairs ? `随机 ${RANDOM_TRANSFER_MIN_NEX}-${RANDOM_TRANSFER_MAX_NEX} NEX` : (args.defaultAmount ? formatNex(args.defaultAmount) : formatNex(BigInt(DEFAULT_TRANSFER_NEX) * NEX_PLANCK)));
    kv('总记录数', `${allRecords.length}`);
    kv('起始索引', `${args.start}`);
    kv('处理上限', args.limit == null ? '不限' : `${args.limit}`);
    kv('随机互转', args.randomPairs ? '是' : '否');
    if (args.randomPairs) {
      kv('停止条件', `成功笔数超过 ${args.randomCount}`);
    }
    kv('Dry Run', args.dryRun ? '是' : '否');
    kv('计划总额', formatNex(totalAmount));

    if (args.randomPairs ? baseRecords.length === 0 : selectedRecords.length === 0) {
      console.log('\n  没有需要处理的记录，退出。');
      return;
    }

    if (args.randomPairs) {
      header('随机互转信息');
      kv('账户数量', `${baseRecords.length}`);
      kv('最低保留余额', formatNex(minimumAccountBalance()));
      kv('目标余额偏好', formatNex(targetAccountBalance()));
    } else {
      const signerInfo = await loadSigner(args);
      const signer = signerInfo.signer;

      header('付款账户信息');
      kv('来源', signerInfo.source);
      kv('地址', signer.address);
      kv('地址(缩写)', shortAddr(signer.address));
      if (signerInfo.actorsFile) {
        kv('账户文件', signerInfo.actorsFile);
      }
      const senderBalance = await readFreeBalance(api, signer.address);
      kv('当前余额', formatNex(senderBalance));
      kv('计划转账总额', formatNex(totalAmount));
    }

    header(args.dryRun ? 'Dry Run 明细' : '执行明细');

    let successCount = 0;
    let sentAmount = 0n;
    let attemptCount = 0;

    if (args.randomPairs) {
      await cryptoWaitReady();
      const balances = await loadInitialBalances(api, baseRecords);
      const maxIdleRounds = Math.max(baseRecords.length * 3, 20);
      let idleRounds = 0;

      while (successCount <= args.randomCount) {
        attemptCount += 1;
        const recipient = pickPreferredRecipient(baseRecords, balances, '');
        if (!recipient) {
          throw new Error('没有可用的收款账户');
        }

        const recipientBalance = balances.get(recipient.address) ?? 0n;
        let executed = false;

        for (const sender of pickRandomSenderCandidates(baseRecords)) {
          if (sender.address === recipient.address) {
            continue;
          }

          const senderBalance = await readFreeBalance(api, sender.address);
          balances.set(sender.address, senderBalance);
          const spendable = senderBalance - minimumAccountBalance();
          let amount = pickRandomTransferAmount(spendable);
          if (amount == null) {
            continue;
          }

          const fromLabel = formatAccountLabel(sender.address, sender.name);
          const toLabel = formatAccountLabel(recipient.address, recipient.name);
          console.log(
            `  [${attemptCount}] ${fromLabel} -> ${toLabel} ${formatNex(amount)}`,
          );

          if (args.dryRun) {
            balances.set(sender.address, senderBalance - amount);
            balances.set(recipient.address, recipientBalance + amount);
            successCount += 1;
            sentAmount += amount;
            executed = true;
            console.log('  [dry-run] planned');
            break;
          }

          const signer = createSignerFromMnemonic(sender.mnemonic!);
          const tx = api.tx.balances.transferKeepAlive(recipient.address, amount.toString());
          const receipt = await submitTx(api, tx, signer, `bulk-transfer-${attemptCount}`);
          if (!receipt.success) {
            console.log(`  [failed] tx=${receipt.txHash} error=${receipt.error ?? 'unknown error'}`);
            throw new Error(`第 ${attemptCount} 条随机互转失败: ${receipt.error ?? 'unknown error'}`);
          }

          successCount += 1;
          sentAmount += amount;
          balances.set(sender.address, senderBalance - amount);
          balances.set(recipient.address, recipientBalance + amount);
          executed = true;
          console.log(
            `  [ok] tx=${receipt.txHash} block=${receipt.blockHash ?? 'n/a'} extrinsic=${receipt.extrinsicIndex ?? 'n/a'}`,
          );
          break;
        }

        if (executed) {
          idleRounds = 0;
          continue;
        }

        idleRounds += 1;
        if (idleRounds >= maxIdleRounds) {
          throw new Error('在当前最小保留余额和金额范围约束下，已经找不到可继续执行的随机转账');
        }
      }
    } else {
      const signerInfo = await loadSigner(args);
      const signer = signerInfo.signer;

      for (const [index, record] of selectedRecords.entries()) {
        const label = formatAccountLabel(record.address, record.name);
        console.log(
          `  [${index + 1}/${selectedRecords.length}] #${record.originalIndex} ${label} -> ${formatNex(record.amount)}`,
        );

        if (args.dryRun) {
          continue;
        }

        const tx = api.tx.balances.transferKeepAlive(record.address, record.amount.toString());
        const receipt = await submitTx(api, tx, signer, `bulk-transfer-${record.originalIndex}`);
        if (!receipt.success) {
          console.log(`  [failed] tx=${receipt.txHash} error=${receipt.error ?? 'unknown error'}`);
          throw new Error(`第 ${record.originalIndex} 条转账失败: ${receipt.error ?? 'unknown error'}`);
        }

        successCount += 1;
        sentAmount += record.amount;
        console.log(
          `  [ok] tx=${receipt.txHash} block=${receipt.blockHash ?? 'n/a'} extrinsic=${receipt.extrinsicIndex ?? 'n/a'}`,
        );
      }
    }

    header('完成');
    kv('尝试条数', `${attemptCount || (args.randomPairs ? successCount : selectedRecords.length)}`);
    kv('成功条数', args.dryRun ? `${successCount} (dry-run)` : `${successCount}`);
    kv('已发送总额', formatNex(sentAmount));
  } finally {
    if (api) {
      await disconnectApi(api);
    }
  }
}

main().catch((error) => {
  console.error(`\n错误: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
});
