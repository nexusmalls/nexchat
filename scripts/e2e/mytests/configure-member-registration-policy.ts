#!/usr/bin/env tsx
/**
 * Query or configure entity member registration policy.
 * 查询或配置实体会员注册策略。
 *
 * Target policy for this task:
 * - purchase required
 * - referral required
 *
 * 本脚本面向本次需求的目标策略：
 * - 必须购买才能成为会员
 * - 必须有推荐人才能注册
 *
 * Usage:
 *   node --import tsx e2e/mytests/configure-member-registration-policy.ts --entity 100000
 *   node --import tsx e2e/mytests/configure-member-registration-policy.ts --shop 100000 --set purchase+referral
 *   node --import tsx e2e/mytests/configure-member-registration-policy.ts --shop 100000 --set 3 --account-file ./e2e/framework/test-accounts-xxxx.json
 *   node --import tsx e2e/mytests/configure-member-registration-policy.ts --shop 100000 --set 3 --account-file ./e2e/framework/test-accounts-xxxx.json --account-index 0
 *
 * Environment:
 *   WS_URL                 WebSocket endpoint
 *   ACCOUNT_FILE           Optional signer JSON account file
 *   ACCOUNT_INDEX          Optional signer index override in JSON account file
 *   MEMBER_POLICY_BITS     Optional policy bits fallback for --set
 */

import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import type { KeyringPair } from '@polkadot/keyring/types';
import { connectApi, disconnectApi, submitTx } from '../framework/api.js';
import { assert, assertTxSuccess } from '../framework/assert.js';
import { codecToHuman, codecToJson } from '../framework/codec.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

const POLICY_PURCHASE_REQUIRED = 0b0000_0001;
const POLICY_REFERRAL_REQUIRED = 0b0000_0010;
const POLICY_APPROVAL_REQUIRED = 0b0000_0100;
const POLICY_KYC_REQUIRED = 0b0000_1000;
const POLICY_KYC_UPGRADE_REQUIRED = 0b0001_0000;

interface CliArgs {
  entityId?: number;
  shopId?: number;
  setBits?: number;
  wsUrl?: string;
  accountFile?: string;
  accountIndex?: number;
}

interface JsonAccountEntry {
  index?: number;
  name?: string;
  mnemonic?: string;
  address?: string;
}

interface JsonAccountFile {
  accounts?: JsonAccountEntry[];
}

function printHelp(): void {
  console.log(`
查询或配置实体会员注册策略。

用法:
  node --import tsx e2e/mytests/configure-member-registration-policy.ts --entity 100000
  node --import tsx e2e/mytests/configure-member-registration-policy.ts --shop 100000 --set purchase+referral

参数:
  --entity <id>            Entity ID
  --shop <id>              Shop ID（脚本会自动解析到 Entity ID）
  --set <value>            设置策略，支持：purchase+referral / 3 / purchase,referral
  --ws <url>               WebSocket 节点地址
  --account-file <path>    签名账户 JSON 文件（设置时用于按 owner 地址自动匹配 signer）
  --account-index <n>      可选：强制使用 JSON 中第 n 个账户签名
  --help, -h               显示帮助

策略位:
  1   PURCHASE_REQUIRED
  2   REFERRAL_REQUIRED
  4   APPROVAL_REQUIRED
  8   KYC_REQUIRED
  16  KYC_UPGRADE_REQUIRED

目标策略:
  purchase+referral = 3
`);
}

function parseArgs(): CliArgs {
  const argv = process.argv.slice(2);
  const args: CliArgs = {
    wsUrl: process.env.WS_URL,
    accountFile: process.env.ACCOUNT_FILE,
    accountIndex: Number(process.env.ACCOUNT_INDEX ?? '0'),
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    switch (arg) {
      case '--help':
      case '-h':
        printHelp();
        process.exit(0);
      case '--entity':
        args.entityId = parseId(argv[++i], '--entity');
        break;
      case '--shop':
        args.shopId = parseId(argv[++i], '--shop');
        break;
      case '--set':
        args.setBits = parsePolicyBits(argv[++i] ?? process.env.MEMBER_POLICY_BITS ?? '');
        break;
      case '--ws':
        args.wsUrl = argv[++i];
        break;
      case '--account-file':
        args.accountFile = argv[++i];
        break;
      case '--account-index':
        args.accountIndex = parseNonNegativeInt(argv[++i], '--account-index');
        break;
      default:
        throw new Error(`未知参数 / Unknown argument: ${arg}`);
    }
  }

  if (args.entityId == null && args.shopId == null) {
    throw new Error('必须提供 --entity 或 --shop / Either --entity or --shop is required');
  }

  return args;
}

function parseId(input: string | undefined, flag: string): number {
  const value = Number(input);
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${flag} 必须是非负整数 / must be a non-negative integer`);
  }
  return value;
}

function parseNonNegativeInt(input: string | undefined, flag: string): number {
  const value = Number(input);
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${flag} 必须是非负整数 / must be a non-negative integer`);
  }
  return value;
}

function parsePolicyBits(raw: string): number {
  const value = raw.trim().toLowerCase();
  if (!value) {
    throw new Error('--set 不能为空 / --set cannot be empty');
  }

  if (/^\d+$/.test(value)) {
    return Number(value);
  }

  const parts = value.split(/[+,]/).map((item) => item.trim()).filter(Boolean);
  let bits = 0;
  for (const part of parts) {
    switch (part) {
      case 'purchase':
      case 'purchase_required':
        bits |= POLICY_PURCHASE_REQUIRED;
        break;
      case 'referral':
      case 'referrer':
      case 'referral_required':
        bits |= POLICY_REFERRAL_REQUIRED;
        break;
      case 'approval':
      case 'approval_required':
        bits |= POLICY_APPROVAL_REQUIRED;
        break;
      case 'kyc':
      case 'kyc_required':
        bits |= POLICY_KYC_REQUIRED;
        break;
      case 'kyc_upgrade':
      case 'kyc_upgrade_required':
        bits |= POLICY_KYC_UPGRADE_REQUIRED;
        break;
      default:
        throw new Error(`无法识别的策略项 / Unknown policy token: ${part}`);
    }
  }
  return bits;
}

function resolvePathMaybe(filePath: string): string {
  return path.isAbsolute(filePath) ? filePath : path.resolve(process.cwd(), filePath);
}

function shortAddr(address: string): string {
  return address.length > 16 ? `${address.slice(0, 8)}...${address.slice(-6)}` : address;
}

function describePolicy(bits: number): string[] {
  const labels: string[] = [];
  if ((bits & POLICY_PURCHASE_REQUIRED) !== 0) labels.push('PURCHASE_REQUIRED');
  if ((bits & POLICY_REFERRAL_REQUIRED) !== 0) labels.push('REFERRAL_REQUIRED');
  if ((bits & POLICY_APPROVAL_REQUIRED) !== 0) labels.push('APPROVAL_REQUIRED');
  if ((bits & POLICY_KYC_REQUIRED) !== 0) labels.push('KYC_REQUIRED');
  if ((bits & POLICY_KYC_UPGRADE_REQUIRED) !== 0) labels.push('KYC_UPGRADE_REQUIRED');
  if (labels.length === 0) labels.push('OPEN');
  return labels;
}

function readPolicyBits(raw: any): number {
  const json = codecToJson(raw);
  if (typeof json === 'number') {
    return json;
  }
  if (typeof json === 'string' && /^\d+$/.test(json)) {
    return Number(json);
  }
  if (raw && typeof raw.toNumber === 'function') {
    return raw.toNumber();
  }
  throw new Error(`无法解析策略位 / Unable to decode policy bits: ${JSON.stringify(json)}`);
}

async function loadAccountFile(accountFile: string): Promise<JsonAccountFile> {
  const resolved = resolvePathMaybe(accountFile);
  const raw = await readFile(resolved, 'utf-8');
  const parsed = JSON.parse(raw) as JsonAccountFile;
  assert(Array.isArray(parsed.accounts), `账户文件缺少 accounts 数组 / Missing accounts array: ${resolved}`);
  return parsed;
}

function buildSignerFromEntry(entry: JsonAccountEntry, keyring: Keyring, fallbackIndex?: number): { signer: KeyringPair; index: number; name: string } {
  const index = entry.index ?? fallbackIndex ?? 0;
  const name = entry.name?.trim() || `account-${index}`;
  assert(entry.mnemonic, `账户缺少 mnemonic / Account missing mnemonic at index ${index}`);

  const signer = keyring.addFromMnemonic(entry.mnemonic);
  if (entry.address && signer.address !== entry.address) {
    throw new Error(`地址校验失败 / Address mismatch: derived=${signer.address} expected=${entry.address}`);
  }

  return { signer, index, name };
}

function selectSignerForOwner(parsed: JsonAccountFile, ownerAddress: string, keyring: Keyring): { signer: KeyringPair; index: number; name: string } {
  const position = parsed.accounts?.findIndex((account) => account.address?.trim() === ownerAddress) ?? -1;
  if (position < 0) {
    throw new Error(`未在账户文件中找到 entity owner ${ownerAddress} / Entity owner ${ownerAddress} was not found in account file`);
  }

  return buildSignerFromEntry(parsed.accounts![position], keyring, position);
}

async function loadSigner(accountFile: string, accountIndex: number | undefined, ownerAddress: string): Promise<{ signer: KeyringPair; index: number; name: string }> {
  const parsed = await loadAccountFile(accountFile);
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });

  if (accountIndex != null) {
    const entry = parsed.accounts?.[accountIndex];
    assert(entry != null, `账户索引越界 / Account index out of range: ${accountIndex}`);
    return buildSignerFromEntry(entry, keyring, accountIndex);
  }

  return selectSignerForOwner(parsed, ownerAddress, keyring);
}

async function resolveEntityId(api: any, args: CliArgs): Promise<{ entityId: number; shopId?: number }> {
  if (args.entityId != null) {
    return { entityId: args.entityId, shopId: args.shopId };
  }

  assert(args.shopId != null, '缺少 shopId / Missing shopId');
  const entityIdCodec = await (api.query as any).entityShop.shopEntityId(args.shopId);
  const entityIdJson = codecToJson(entityIdCodec);

  if (typeof entityIdJson === 'number') {
    return { entityId: entityIdJson, shopId: args.shopId };
  }
  if (typeof entityIdJson === 'string' && /^\d+$/.test(entityIdJson)) {
    return { entityId: Number(entityIdJson), shopId: args.shopId };
  }
  if (entityIdCodec?.isSome === true) {
    const inner = entityIdCodec.unwrap();
    if (typeof inner.toNumber === 'function') {
      return { entityId: inner.toNumber(), shopId: args.shopId };
    }
  }

  throw new Error(`无法通过 shop ${args.shopId} 解析 entity / Failed to resolve entity from shop ${args.shopId}`);
}

async function resolveEntityOwnerAddress(api: any, entityId: number): Promise<string> {
  const entityCodec = await (api.query as any).entityRegistry.entities(entityId);
  if (entityCodec?.isNone === true) {
    throw new Error(`entity ${entityId} 不存在 / Entity ${entityId} not found`);
  }

  const entityJson = codecToJson(entityCodec?.isSome === true ? entityCodec.unwrap() : entityCodec);
  const owner = typeof entityJson === 'object' && entityJson != null ? (entityJson as Record<string, unknown>).owner : undefined;
  const ownerAddress = typeof owner === 'string' ? owner.trim() : '';
  if (!ownerAddress) {
    throw new Error(`无法读取 entity ${entityId} 的 owner 地址 / Failed to read owner address for entity ${entityId}`);
  }
  return ownerAddress;
}

async function main(): Promise<void> {
  const args = parseArgs();
  await cryptoWaitReady();

  const api = await connectApi(args.wsUrl);
  try {
    const { entityId, shopId } = await resolveEntityId(api, args);
    const memberQuery = (api.query as any).entityMember;

    const beforeCodec = await memberQuery.entityMemberPolicy(entityId);
    const beforeBits = readPolicyBits(beforeCodec);

    console.log('=== Member Registration Policy ===');
    console.log(`WS_URL:       ${args.wsUrl ?? process.env.WS_URL ?? 'ws://127.0.0.1:9944'}`);
    console.log(`Entity ID:    ${entityId}`);
    console.log(`Shop ID:      ${shopId ?? '(not provided)'}`);
    console.log(`Current bits: ${beforeBits}`);
    console.log(`Current flags:${describePolicy(beforeBits).join(' + ')}`);
    console.log(`Current human:${JSON.stringify(codecToHuman(beforeCodec))}`);

    if (args.setBits == null) {
      console.log('\n只查询，未修改链状态。');
      return;
    }

    assert(shopId != null, '设置策略时必须提供 --shop，因为 extrinsic 为 setMemberPolicy(shop_id, policy_bits) / --shop is required when setting policy');

    const ownerAddress = await resolveEntityOwnerAddress(api, entityId);
    console.log(`Entity owner: ${shortAddr(ownerAddress)}`);

    const signerFile = args.accountFile;
    assert(signerFile, '设置策略时必须提供 --account-file 或环境变量 ACCOUNT_FILE / --account-file or ACCOUNT_FILE is required when setting policy');

    const selected = await loadSigner(signerFile, args.accountIndex, ownerAddress);
    const signer = selected.signer;
    console.log(`Signer:       ${shortAddr(signer.address)} (${selected.name}, index=${selected.index})`);
    if (signer.address !== ownerAddress) {
      throw new Error(`签名账户不是 entity owner / Signer is not the entity owner: signer=${signer.address} owner=${ownerAddress}`);
    }
    console.log(`Target bits:  ${args.setBits}`);
    console.log(`Target flags: ${describePolicy(args.setBits).join(' + ')}`);

    const tx = (api.tx as any).entityMember.setMemberPolicy(shopId, args.setBits);
    const receipt = await submitTx(api, tx, signer, 'set-member-policy');
    assertTxSuccess(receipt, '设置会员注册策略失败 / set member registration policy failed');

    const afterCodec = await memberQuery.entityMemberPolicy(entityId);
    const afterBits = readPolicyBits(afterCodec);

    console.log('\n=== Updated Policy ===');
    console.log(`After bits:   ${afterBits}`);
    console.log(`After flags:  ${describePolicy(afterBits).join(' + ')}`);
    console.log(`After human:  ${JSON.stringify(codecToHuman(afterCodec))}`);

    assert(afterBits === args.setBits, `链上策略校验失败 / on-chain policy mismatch: expected=${args.setBits} actual=${afterBits}`);
    console.log('\n已成功更新会员注册策略。');
  } finally {
    await disconnectApi(api);
  }
}

main().catch((error) => {
  console.error('Error:', error instanceof Error ? error.message : error);
  process.exit(1);
});
