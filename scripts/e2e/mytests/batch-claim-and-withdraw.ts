#!/usr/bin/env tsx
/**
 * 批量执行沉淀奖金池领奖 + NEX 佣金提现
 *
 * 用法:
 *   npx tsx batch-claim-and-withdraw.ts \
 *     --entity 100010 \
 *     --files file1.json,file2.json,file3.json \
 *     [--ws wss://rpc.nexusmall.net] \
 *     [--retry-file reports/failed-accounts.json]
 *
 * 说明:
 *   - 对每个文件中的全部账户，先执行 entity 沉淀奖金池领奖
 *   - 再执行 NEX 佣金提现（不带 --token）
 *   - 单个账户失败不会中断整体流程
 *   - 失败账户会导出到 retry 文件，便于下次只重跑失败账户
 */

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

interface CliArgs {
  entityId: number;
  files: string[];
  wsUrl: string;
  retryFile: string;
}

interface AccountEntry {
  index: number;
  name?: string;
  mnemonic: string;
  address: string;
}

interface AccountFile {
  network?: string;
  accounts: AccountEntry[];
}

interface RetryAccountEntry extends AccountEntry {
  sourceFile: string;
}

interface RetryFileContent {
  entityId: number;
  wsUrl: string;
  generatedAt: string;
  accounts: RetryAccountEntry[];
}

interface CommandResult {
  ok: boolean;
  code: number | null;
}

interface SummaryItem {
  file: string;
  account: string;
  claimOk: boolean;
  withdrawOk: boolean;
  accountEntry: RetryAccountEntry;
}

function printHelp(): void {
  console.log(`
批量执行沉淀奖金池领奖 + NEX 佣金提现

用法:
  npx tsx batch-claim-and-withdraw.ts --entity 100010 --files file1.json,file2.json,file3.json [--ws wss://rpc.nexusmall.net] [--retry-file reports/failed-accounts.json]

必填参数:
  --entity, -e      实体 ID
  --files,  -f      账户文件列表，逗号分隔；也可以传入上次导出的 retry JSON

可选参数:
  --ws              WebSocket 地址
  --retry-file      失败账户导出文件 (默认 reports/failed-accounts.json)
  --help,   -h      显示帮助

示例:
  npx tsx batch-claim-and-withdraw.ts \
    -e 100010 \
    -f ./test-accounts-a.json,./test-accounts-b.json \
    --ws wss://rpc.nexusmall.net

  npx tsx batch-claim-and-withdraw.ts \
    -e 100010 \
    -f ./reports/failed-accounts.json
`);
}

function parseArgs(): CliArgs {
  const argv = process.argv.slice(2);

  if (argv.includes('--help') || argv.includes('-h') || argv.length === 0) {
    printHelp();
    process.exit(0);
  }

  let entityId = NaN;
  let files: string[] = [];
  let wsUrl = 'wss://rpc.nexusmall.net';
  let retryFile = 'reports/failed-accounts.json';

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case '--entity':
      case '-e':
        entityId = Number(argv[++i]);
        break;
      case '--files':
      case '-f':
        files = (argv[++i] ?? '').split(',').map((item) => item.trim()).filter(Boolean);
        break;
      case '--ws':
        wsUrl = argv[++i] ?? wsUrl;
        break;
      case '--retry-file':
        retryFile = argv[++i] ?? retryFile;
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

  if (files.length === 0) {
    console.error('错误: 必须提供至少一个 --files 文件');
    process.exit(1);
  }

  return { entityId, files, wsUrl, retryFile };
}

function ln(char = '─', len = 76): string { return char.repeat(len); }

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
  console.log(`  ${label.padEnd(22)} ${value}`);
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

async function loadAccounts(filePath: string): Promise<AccountFile> {
  const raw = await readFile(filePath, 'utf-8');
  const parsed = JSON.parse(raw) as AccountFile;
  if (!Array.isArray(parsed.accounts)) {
    throw new Error(`账户文件缺少 accounts 数组: ${filePath}`);
  }
  return parsed;
}

async function loadRetryAccounts(filePath: string): Promise<RetryAccountEntry[]> {
  const raw = await readFile(filePath, 'utf-8');
  const parsed = JSON.parse(raw) as RetryFileContent;
  if (!Array.isArray(parsed.accounts)) {
    throw new Error(`retry 文件缺少 accounts 数组: ${filePath}`);
  }
  return parsed.accounts;
}

function runCommand(command: string, args: string[], env: NodeJS.ProcessEnv): Promise<CommandResult> {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      stdio: 'inherit',
      env,
    });

    child.on('close', (code) => {
      resolve({ ok: code === 0, code });
    });

    child.on('error', () => {
      resolve({ ok: false, code: null });
    });
  });
}

async function writeRetryFile(filePath: string, entityId: number, wsUrl: string, accounts: RetryAccountEntry[]): Promise<void> {
  const absPath = path.resolve(filePath);
  await mkdir(path.dirname(absPath), { recursive: true });
  await writeFile(absPath, JSON.stringify({
    entityId,
    wsUrl,
    generatedAt: new Date().toISOString(),
    accounts,
  }, null, 2) + '\n', 'utf-8');
}

async function expandInputFiles(files: string[]): Promise<RetryAccountEntry[]> {
  const expanded: RetryAccountEntry[] = [];

  for (const rawFile of files) {
    const filePath = path.resolve(rawFile);
    if (filePath.endsWith('.json') && path.basename(filePath).includes('failed-accounts')) {
      const retryAccounts = await loadRetryAccounts(filePath);
      expanded.push(...retryAccounts);
      continue;
    }

    const accountFile = await loadAccounts(filePath);
    for (const account of accountFile.accounts) {
      expanded.push({ ...account, sourceFile: path.basename(filePath) });
    }
  }

  return expanded;
}

async function main(): Promise<void> {
  const args = parseArgs();
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const claimScript = path.resolve(scriptDir, 'claim-pool-reward.ts');
  const withdrawScript = path.resolve(scriptDir, 'withdraw-commission.ts');
  const accounts = await expandInputFiles(args.files);

  header('批量领奖 + 提现');
  kv('实体 ID', `${args.entityId}`);
  kv('节点', args.wsUrl);
  kv('输入文件数', `${args.files.length}`);
  kv('账户数量', `${accounts.length}`);
  kv('失败导出', path.resolve(args.retryFile));

  const summary: SummaryItem[] = [];

  for (const account of accounts) {
    subHeader(`账户 ${account.index} · ${account.name ?? 'unnamed'} · ${shortAddr(account.address)} · ${account.sourceFile}`);

    const env = {
      ...process.env,
      WS_URL: args.wsUrl,
    };

    console.log('  [1/2] 执行沉淀奖金池领奖...');
    const claimResult = await runCommand(
      'npx',
      ['tsx', claimScript, '--mnemonic', account.mnemonic, '--entity', String(args.entityId), '--ws', args.wsUrl],
      env,
    );
    console.log(`  领奖结果: ${claimResult.ok ? '成功' : `失败(code=${claimResult.code ?? 'null'})`}`);

    console.log('  [2/2] 执行 NEX 佣金提现...');
    const withdrawResult = await runCommand(
      'npx',
      ['tsx', withdrawScript, '--mnemonic', account.mnemonic, '--entity', String(args.entityId), '--ws', args.wsUrl],
      env,
    );
    console.log(`  提现结果: ${withdrawResult.ok ? '成功' : `失败(code=${withdrawResult.code ?? 'null'})`}`);

    summary.push({
      file: account.sourceFile,
      account: account.address,
      claimOk: claimResult.ok,
      withdrawOk: withdrawResult.ok,
      accountEntry: account,
    });
  }

  header('执行汇总');
  const total = summary.length;
  const claimSuccess = summary.filter((item) => item.claimOk).length;
  const withdrawSuccess = summary.filter((item) => item.withdrawOk).length;
  kv('总账户数', `${total}`);
  kv('领奖成功', `${claimSuccess}/${total}`);
  kv('提现成功', `${withdrawSuccess}/${total}`);

  const failed = summary.filter((item) => !item.claimOk || !item.withdrawOk);
  if (failed.length > 0) {
    console.log('\n  失败明细:');
    for (const item of failed) {
      console.log(`    ${item.file} | ${item.account} | 领奖=${item.claimOk ? 'OK' : 'FAIL'} | 提现=${item.withdrawOk ? 'OK' : 'FAIL'}`);
    }

    await writeRetryFile(
      args.retryFile,
      args.entityId,
      args.wsUrl,
      failed.map((item) => item.accountEntry),
    );
    kv('失败账户已导出', path.resolve(args.retryFile));
  } else {
    await writeRetryFile(args.retryFile, args.entityId, args.wsUrl, []);
    kv('失败账户已导出', `${path.resolve(args.retryFile)} (空)`);
  }

  console.log(`\n${ln('═')}`);
  console.log('  批量执行完成!');
  console.log(ln('═') + '\n');
}

main().catch((err) => {
  console.error('错误:', err.message ?? err);
  process.exit(1);
});
