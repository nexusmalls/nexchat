#!/usr/bin/env tsx
/**
 * generate-test-accounts.ts
 *
 * 生成测试账户 JSON 文件，包含助记词、地址、公钥，以及可选备注。
 * Generate test account JSON files with mnemonic, address, public key, and optional notes.
 *
 * 用法 / Usage:
 *   npx tsx mytests/generate-test-accounts.ts [--count 20] [--network wss://nexuscom.duckdns.org:9948] [--output path]
 *
 * 参数 / Options:
 *   --count    账户数量（默认 20）
 *   --network  链 RPC 端点（默认 wss://nexuscom.duckdns.org:9948）
 *   --output   输出文件路径（默认自动生成带时间戳的文件名）
 *
 * 输出文件中每个账户都有 note 字段，生成后可手动编辑填写备注。
 */

import { cryptoWaitReady, mnemonicGenerate, encodeAddress } from '@polkadot/util-crypto';
import { Keyring } from '@polkadot/keyring';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

// ─── 参数解析 ───

function parseArgs(): { count: number; network: string; output: string } {
  const args = process.argv.slice(2);
  let count = 20;
  let network = 'wss://nexuscom.duckdns.org:9948';
  let output = '';

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case '--count':
        count = Number(args[++i]);
        if (!Number.isInteger(count) || count < 1) {
          console.error('ERROR: --count must be a positive integer');
          process.exit(1);
        }
        break;
      case '--network':
        network = args[++i];
        break;
      case '--output':
        output = args[++i];
        break;
    }
  }

  if (!output) {
    const scriptDir = path.dirname(fileURLToPath(import.meta.url));
    const ts = new Date().toISOString().replace(/[:.]/g, '-');
    output = path.resolve(scriptDir, `test-accounts-${ts}.json`);
  } else if (!path.isAbsolute(output)) {
    output = path.resolve(process.cwd(), output);
  }

  return { count, network, output };
}

// ─── 账户生成 ───

interface AccountEntry {
  index: number;
  name: string;
  mnemonic: string;
  address: string;
  publicKey: string;
  note: string;
}

interface AccountFile {
  createdAt: string;
  network: string;
  ss58Format: number;
  accountCount: number;
  _usage: string;
  accounts: AccountEntry[];
}

async function generateAccounts(count: number, network: string): Promise<AccountFile> {
  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const accounts: AccountEntry[] = [];

  for (let i = 0; i < count; i++) {
    const mnemonic = mnemonicGenerate(12);
    const pair = keyring.addFromMnemonic(mnemonic);
    const publicKeyHex = Buffer.from(pair.publicKey).toString('hex');

    accounts.push({
      index: i,
      name: `account-${i}`,
      mnemonic,
      address: pair.address,
      publicKey: publicKeyHex,
      note: '',
    });
  }

  return {
    createdAt: new Date().toISOString(),
    network,
    ss58Format: NEXUS_SS58_FORMAT,
    accountCount: count,
    _usage: '每个账户的 note 字段可手动填写备注（用途、角色等）。Edit the "note" field to add remarks for each account.',
    accounts,
  };
}

// ─── 主流程 ───

async function main(): Promise<void> {
  const { count, network, output } = parseArgs();

  console.log(`\n${'═'.repeat(68)}`);
  console.log(`  Nexus 测试账户生成器 / Test Account Generator`);
  console.log(`${'═'.repeat(68)}`);
  console.log(`  数量 / Count:    ${count}`);
  console.log(`  网络 / Network:  ${network}`);
  console.log(`  SS58 Prefix:     ${NEXUS_SS58_FORMAT}`);
  console.log(`  输出 / Output:   ${output}`);
  console.log(`${'─'.repeat(68)}\n`);

  const data = await generateAccounts(count, network);

  await writeFile(output, JSON.stringify(data, null, 2) + '\n', 'utf-8');

  console.log(`  生成完成 / Generated ${count} accounts\n`);
  console.log(`  ${'Idx'.padEnd(5)} ${'Name'.padEnd(14)} ${'Address'.padEnd(52)} Note`);
  console.log(`  ${'─'.repeat(64)}`);
  for (const a of data.accounts) {
    console.log(`  ${String(a.index).padEnd(5)} ${a.name.padEnd(14)} ${a.address}`);
  }

  console.log(`\n${'─'.repeat(68)}`);
  console.log(`  文件已保存 / Saved to: ${output}`);
  console.log(`  提示: 编辑 JSON 中的 "note" 字段来添加备注`);
  console.log(`  Tip:  Edit the "note" field in the JSON to add remarks`);
  console.log(`${'═'.repeat(68)}\n`);
}

main().catch((e: unknown) => {
  console.error('\n[ERROR]', e instanceof Error ? e.message : String(e));
  process.exit(1);
});
