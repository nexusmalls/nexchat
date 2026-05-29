#!/usr/bin/env tsx
/**
 * Create two entities from a JSON account file and fund their primary shops.
 * 从 JSON 账户文件创建两个实体，并为各自的主店铺注入经营资金。
 */

import { readFile } from 'node:fs/promises';
import { createInterface } from 'node:readline';
import path from 'node:path';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import type { KeyringPair } from '@polkadot/keyring/types';
import { connectApi, disconnectApi } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { formatNex, NEX_PLANCK } from '../framework/units.js';
import { readEntityIds, setupFreshEntity } from '../suites/helpers.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

const DEFAULT_ACCOUNT_FILE = '/home/xiaodong/桌面/nexus/scripts/e2e/mytests/test-accounts-2026-04-14T03-13-20-529Z（用于注册entity的账户）.json';
const DEFAULT_SHOP_FUND = parseNexToPlanck('2000', '--shop-fund');
const DEFAULT_MIN_BALANCE = parseNexToPlanck('3000', '--min-balance');

interface CliArgs {
  filePath: string;
  wsUrl?: string;
  shopFund: bigint;
  minBalance: bigint;
}

interface JsonAccountEntry {
  index?: number;
  name?: string;
  mnemonic?: string;
  address?: string;
}

interface JsonAccountFile {
  network?: string;
  accounts?: JsonAccountEntry[];
}

interface SelectedAccount {
  index: number;
  name: string;
  expectedAddress: string;
  signer: KeyringPair;
  initialBalance: bigint;
  finalBalance?: bigint;
  existingEntityIds: number[];
  createdEntityIds: number[];
  createdShopIds: number[];
  requiredBalance: bigint;
  missingCount: number;
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
  console.log(`  ${label.padEnd(18)} ${value}`);
}

function log(tag: string, message: string): void {
  const ts = new Date().toISOString().slice(11, 23);
  console.log(`[${ts}] [${tag}] ${message}`);
}

async function waitForEnter(prompt: string): Promise<void> {
  if (process.stdin.isTTY !== true) {
    console.log(`\n  [检测到非交互模式，自动继续] ${prompt}`);
    return;
  }

  await new Promise<void>((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    rl.question(`\n  ▶ ${prompt}\n     [按 Enter 继续] `, () => {
      rl.close();
      resolve();
    });
  });
}

function printHelp(): void {
  console.log(`
按账户补足到两个 Entity：处理 JSON 文件里的所有账户，每个账户最多创建到 2 个 entity，
并为新建 entity 的自动主店铺注入经营资金。

用法:
  node --import tsx e2e/mytests/create-two-entities-from-json.ts [选项]

可选参数:
  --file, -f        账户 JSON 文件路径
                    默认: ${DEFAULT_ACCOUNT_FILE}
  --ws              WebSocket 节点地址；默认使用 JSON 文件中的 network 字段
  --shop-fund       每个新建主店铺注资 NEX 数量，默认 2000
  --min-balance     每个待创建 entity 的基础最低余额，默认 3000
  --help, -h        显示帮助

说明:
  - 处理 JSON 文件里的所有账户
  - 每个账户补足到 2 个 entity；已有 2 个及以上则跳过
  - 不创建产品
  - 不自动补余额；若余额不足，会在提交该账户交易前直接失败退出
`);
}

function parseArgs(): CliArgs {
  const argv = process.argv.slice(2);
  let filePath = DEFAULT_ACCOUNT_FILE;
  let wsUrl: string | undefined;
  let shopFund = DEFAULT_SHOP_FUND;
  let minBalance = DEFAULT_MIN_BALANCE;

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    switch (arg) {
      case '--help':
      case '-h':
        printHelp();
        process.exit(0);
      case '--file':
      case '-f':
        filePath = argv[++i] ?? '';
        break;
      case '--ws':
        wsUrl = argv[++i] ?? '';
        break;
      case '--shop-fund':
        shopFund = parseNexToPlanck(argv[++i] ?? '', '--shop-fund');
        break;
      case '--min-balance':
        minBalance = parseNexToPlanck(argv[++i] ?? '', '--min-balance');
        break;
      default:
        throw new Error(`未知参数: ${arg}`);
    }
  }

  if (!filePath.trim()) {
    throw new Error('--file 不能为空');
  }
  if (wsUrl != null && !wsUrl.trim()) {
    throw new Error('--ws 不能为空');
  }

  return { filePath, wsUrl, shopFund, minBalance };
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
  const fraction = BigInt(fractionPart.padEnd(12, '0') || '0');
  const total = whole + fraction;

  if (total <= 0n) {
    throw new Error(`${field} 必须大于 0，当前值: ${raw}`);
  }

  return total;
}

function resolveFilePath(filePath: string): string {
  return path.isAbsolute(filePath) ? filePath : path.resolve(process.cwd(), filePath);
}

async function loadAccountFile(filePath: string): Promise<JsonAccountFile> {
  log('文件', `正在读取账户文件: ${filePath}`);
  const raw = await readFile(filePath, 'utf-8');
  const parsed = JSON.parse(raw) as JsonAccountFile;
  if (!Array.isArray(parsed.accounts)) {
    throw new Error(`账户文件 ${filePath} 缺少有效的 accounts 数组`);
  }
  if (parsed.accounts.length < 2) {
    throw new Error(`账户文件 ${filePath} 仅包含 ${parsed.accounts.length} 个账户，至少需要 2 个`);
  }
  log('文件', `账户文件读取成功，共发现 ${parsed.accounts.length} 个账户`);
  return parsed;
}

function selectSigners(parsed: JsonAccountFile): SelectedAccount[] {
  log('账户', '正在初始化加密库并派生 JSON 文件中的全部账户');
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });

  return parsed.accounts!.map((account, position) => {
    const name = account.name?.trim() || `account-${position}`;
    const index = account.index ?? position;
    const mnemonic = account.mnemonic?.trim();
    const expectedAddress = account.address?.trim();

    if (!mnemonic) {
      throw new Error(`accounts[${position}] 的账户 ${name} 缺少 mnemonic`);
    }
    if (!expectedAddress) {
      throw new Error(`accounts[${position}] 的账户 ${name} 缺少 address`);
    }

    const signer = keyring.addFromMnemonic(mnemonic);
    if (signer.address !== expectedAddress) {
      throw new Error(`账户 ${name} 地址校验失败：派生地址 ${signer.address}，JSON 地址 ${expectedAddress}`);
    }

    log('账户', `已载入 ${name} [index=${index}] ${expectedAddress}`);

    return {
      index,
      name,
      expectedAddress,
      signer,
      initialBalance: 0n,
      existingEntityIds: [],
      createdEntityIds: [],
      createdShopIds: [],
      requiredBalance: 0n,
      missingCount: 0,
    };
  });
}

async function preflightBalances(accounts: SelectedAccount[], minBalance: bigint): Promise<void> {
  header('余额与缺口预检查');
  const insufficient: string[] = [];

  for (const account of accounts) {
    log('余额', `账户 ${account.name} 当前可用余额: ${formatNex(account.initialBalance)}`);
    log('实体', `账户 ${account.name} 当前已有 ${account.existingEntityIds.length} 个 entity，仍需补建 ${account.missingCount} 个`);
    if (account.missingCount === 0) {
      continue;
    }
    if (account.initialBalance >= account.requiredBalance) {
      continue;
    }
    insufficient.push(
      `${account.name} (${account.signer.address}) 当前余额=${formatNex(account.initialBalance)}，需补建=${account.missingCount}，要求至少=${formatNex(account.requiredBalance)}，缺口=${formatNex(account.requiredBalance - account.initialBalance)}`,
    );
  }

  if (insufficient.length > 0) {
    throw new Error(`账户余额不足，未提交任何交易：\n${insufficient.map((line) => `  - ${line}`).join('\n')}`);
  }

  log('余额', `所有需要补建的账户都满足余额要求（基础阈值: ${formatNex(minBalance)} / 每个待建 entity）`);
}

function printConfig(filePath: string, wsUrl: string, shopFund: bigint, minBalance: bigint, accounts: SelectedAccount[]): void {
  header('执行配置');
  kv('账户文件', filePath);
  kv('节点地址', wsUrl);
  kv('账户总数', String(accounts.length));
  kv('目标规则', '每个账户补足到 2 个 entity，已有 2 个及以上则跳过');
  kv('单店铺注资', formatNex(shopFund));
  kv('基础最低余额', `${formatNex(minBalance)} / 每个待建 entity`);
  kv('产品创建', '不创建产品，仅准备 entity 和主店铺');
  console.log(`\n${ln()}`);
  console.log('  将处理以下账户：');
  for (const account of accounts) {
    kv(`${account.name}`, `[index=${account.index}] ${account.signer.address}`);
  }
}

function printSummary(accounts: SelectedAccount[]): void {
  const createdAccounts = accounts.filter((account) => account.createdEntityIds.length > 0).length;
  const skippedAccounts = accounts.filter((account) => account.missingCount === 0).length;
  const createdTotal = accounts.reduce((sum, account) => sum + account.createdEntityIds.length, 0);

  header('执行结果汇总');
  kv('账户总数', String(accounts.length));
  kv('已满足跳过', String(skippedAccounts));
  kv('实际补建账户', String(createdAccounts));
  kv('新建 entity 总数', String(createdTotal));

  for (const account of accounts) {
    const finalCount = account.existingEntityIds.length + account.createdEntityIds.length;
    console.log(`\n  ${account.name} [index=${account.index}]`);
    kv('地址', account.signer.address);
    kv('原有 entity 数', String(account.existingEntityIds.length));
    kv('本次新建数', String(account.createdEntityIds.length));
    kv('最终 entity 数', String(finalCount));
    kv('原有 entityIds', account.existingEntityIds.length > 0 ? account.existingEntityIds.join(', ') : '无');
    kv('新建 entityIds', account.createdEntityIds.length > 0 ? account.createdEntityIds.join(', ') : '无');
    kv('新建 shopIds', account.createdShopIds.length > 0 ? account.createdShopIds.join(', ') : '无');
    kv('初始余额', formatNex(account.initialBalance));
    kv('结束余额', account.finalBalance != null ? formatNex(account.finalBalance) : '未知');
  }
}

async function main(): Promise<void> {
  header('按账户补足到两个 Entity 脚本');
  const args = parseArgs();
  log('初始化', '命令行参数解析完成');

  const filePath = resolveFilePath(args.filePath);
  const parsed = await loadAccountFile(filePath);
  const wsUrl = args.wsUrl ?? parsed.network;

  if (!wsUrl) {
    throw new Error('未提供 --ws，且账户 JSON 文件中也没有 network 字段');
  }

  log('初始化', '正在初始化加密组件');
  await cryptoWaitReady();
  const accounts = selectSigners(parsed);
  printConfig(filePath, wsUrl, args.shopFund, args.minBalance, accounts);
  await waitForEnter('请确认账户文件、节点地址、补足规则、店铺注资金额和余额阈值无误');

  log('连接', `正在连接链节点: ${wsUrl}`);
  const api = await connectApi(wsUrl);
  try {
    header('读取现状与初始余额');
    for (const account of accounts) {
      account.initialBalance = await readFreeBalance(api, account.signer.address);
      account.existingEntityIds = await readEntityIds(api, account.signer.address);
      account.missingCount = Math.max(0, 2 - account.existingEntityIds.length);
      account.requiredBalance = args.minBalance * BigInt(account.missingCount);
      log('账户', `${account.name} 当前已有 entityIds: ${account.existingEntityIds.length > 0 ? account.existingEntityIds.join(', ') : '无'}`);
      log('账户', `${account.name} 初始可用余额: ${formatNex(account.initialBalance)}，需补建 ${account.missingCount} 个，要求余额 ${formatNex(account.requiredBalance)}`);
    }

    await preflightBalances(accounts, args.minBalance);

    for (const [position, account] of accounts.entries()) {
      header(`处理账户 ${position + 1}/${accounts.length}: ${account.name}`);
      kv('账户地址', account.signer.address);
      kv('原有 entity 数', String(account.existingEntityIds.length));
      kv('需补建数量', String(account.missingCount));
      kv('计划注资', formatNex(args.shopFund));
      kv('当前余额', formatNex(account.initialBalance));

      if (account.missingCount === 0) {
        log('跳过', `${account.name} 已有 ${account.existingEntityIds.length} 个 entity，不再创建`);
        account.finalBalance = account.initialBalance;
        continue;
      }

      await waitForEnter(`即将为 ${account.name} 补建 ${account.missingCount} 个 entity，并为每个新主店铺注资 ${formatNex(args.shopFund)}`);

      for (let i = 0; i < account.missingCount; i += 1) {
        log('交易', `开始为 ${account.name} 创建第 ${i + 1}/${account.missingCount} 个缺失 entity`);
        const created = await setupFreshEntity(api, account.signer, args.shopFund);
        account.createdEntityIds.push(created.entityId);
        account.createdShopIds.push(created.shopId);
        log('成功', `${account.name} 新建完成：entityId=${created.entityId}, 主店铺 shopId=${created.shopId}`);
      }

      account.finalBalance = await readFreeBalance(api, account.signer.address);
      log('余额', `${account.name} 结束可用余额: ${formatNex(account.finalBalance)}`);
    }

    printSummary(accounts);
  } finally {
    log('清理', '正在断开链节点连接');
    await disconnectApi(api);
  }
}

main().catch((error) => {
  console.error(`\n错误: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
