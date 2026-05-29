import type { ApiPromise } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { stat, readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { submitTx } from './api.js';
import { assertTxSuccess } from './assert.js';
import { DevActors } from './types.js';
import { nex } from './units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

const FRAMEWORK_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_WS_URL = 'ws://127.0.0.1:9944';
const ACTOR_ROLE_NAMES = ['alice', 'bob', 'charlie', 'dave', 'eve', 'ferdie'] as const;

type ActorRoleName = typeof ACTOR_ROLE_NAMES[number];

interface JsonAccountEntry {
  mnemonic: string;
  address: string;
  name?: string;
  publicKey?: string;
}

interface JsonAccountFile {
  createdAt?: string;
  network?: string;
  accountCount?: number;
  accounts: JsonAccountEntry[];
}

let keyring: Keyring | undefined;
let cryptoReadyPromise: Promise<boolean> | undefined;
let selectedActorsFilePath: string | undefined;

/**
 * 获取全局复用的 Keyring，统一使用 Nexus 的地址格式。
 */
function getKeyring(): Keyring {
  keyring ??= new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  return keyring;
}

/**
 * 读取当前链连接地址，未设置时回退到本地开发链地址。
 */
function currentWsUrl(): string {
  return process.env.WS_URL ?? DEFAULT_WS_URL;
}

/**
 * 读取用户显式指定的测试账户文件覆盖配置。
 */
function actorFileOverride(): string | undefined {
  const value = process.env.E2E_ACTORS_FILE?.trim();
  return value ? value : undefined;
}

/**
 * 将账户文件路径解析成绝对路径，兼容传入绝对路径和文件名。
 */
function resolveActorFilePath(input: string): string {
  return path.isAbsolute(input) ? input : path.resolve(FRAMEWORK_DIR, input);
}

/**
 * 读取并校验测试账户 JSON 文件的基本结构。
 */
async function loadJsonAccountFile(filePath: string): Promise<JsonAccountFile> {
  const raw = await readFile(filePath, 'utf-8');
  const parsed = JSON.parse(raw) as JsonAccountFile;
  if (!Array.isArray(parsed.accounts)) {
    throw new Error(`Actor file ${filePath} is missing a valid accounts array`);
  }
  return parsed;
}

/**
 * 按 WS_URL 匹配并选择最新的测试账户文件。
 */
async function findMatchingActorFile(wsUrl: string): Promise<string> {
  const entries = await readdir(FRAMEWORK_DIR);
  const candidates = entries
    .filter((entry) => /^test-accounts-.*\.json$/.test(entry))
    .map((entry) => path.resolve(FRAMEWORK_DIR, entry));

  if (candidates.length === 0) {
    throw new Error(`No test-accounts-*.json files found in ${FRAMEWORK_DIR}`);
  }

  const matches: Array<{ filePath: string; mtimeMs: number }> = [];
  for (const filePath of candidates) {
    const parsed = await loadJsonAccountFile(filePath);
    if ((parsed.network ?? DEFAULT_WS_URL) !== wsUrl) {
      continue;
    }
    const metadata = await stat(filePath);
    matches.push({ filePath, mtimeMs: metadata.mtimeMs });
  }

  if (matches.length === 0) {
    throw new Error(`No actor file in ${FRAMEWORK_DIR} matches WS_URL=${wsUrl}`);
  }

  matches.sort((left, right) => right.mtimeMs - left.mtimeMs || right.filePath.localeCompare(left.filePath));
  return matches[0].filePath;
}

/**
 * 选择实际要使用的测试账户文件，优先使用显式覆盖配置。
 */
async function selectActorsFilePath(): Promise<string> {
  const override = actorFileOverride();
  if (override) {
    return resolveActorFilePath(override);
  }
  return findMatchingActorFile(currentWsUrl());
}

/**
 * 确保加密组件初始化完成，避免后续派生账户失败。
 */
async function ensureCryptoReady(): Promise<void> {
  cryptoReadyPromise ??= cryptoWaitReady();
  await cryptoReadyPromise;
}

/**
 * 加载开发测试账户并按固定角色顺序映射到 actors。
 */
export async function getDevActors(): Promise<DevActors> {
  await ensureCryptoReady();
  const keyring = getKeyring();
  const filePath = await selectActorsFilePath();
  const parsed = await loadJsonAccountFile(filePath);

  if (parsed.accounts.length < ACTOR_ROLE_NAMES.length) {
    throw new Error(`Actor file ${filePath} has ${parsed.accounts.length} accounts; need at least ${ACTOR_ROLE_NAMES.length}`);
  }

  selectedActorsFilePath = filePath;

  const actors = {} as DevActors;
  for (const [index, role] of ACTOR_ROLE_NAMES.entries()) {
    const account = parsed.accounts[index];
    if (!account?.mnemonic) {
      throw new Error(`Actor file ${filePath} is missing mnemonic for role ${role} at accounts[${index}]`);
    }
    actors[role] = keyring.addFromMnemonic(account.mnemonic);
  }
  return actors;
}

/**
 * 返回当前已选中的测试账户文件路径，供启动日志展示。
 */
export function getSelectedActorsFilePath(): string | undefined {
  return selectedActorsFilePath;
}

/**
 * 读取指定地址的可用余额。
 */
export async function readFreeBalance(api: ApiPromise, address: string): Promise<bigint> {
  const account = await api.query.system.account(address);
  return BigInt(((account as any).data.free as any).toString());
}

/**
 * 确保所有测试账户都满足最低余额要求。
 */
export async function ensureActorBalance(api: ApiPromise, actors: DevActors, minNex: number): Promise<void> {
  await ensureNamedActorBalance(api, actors, Object.keys(actors), minNex);
}

/**
 * 仅为指定名称的测试账户补足余额，不足时由 alice 作为水龙头转账。
 */
export async function ensureNamedActorBalance(
  api: ApiPromise,
  actors: DevActors,
  actorNames: string[],
  minNex: number,
): Promise<void> {
  const minimum = nex(minNex);
  const faucet = actors.alice;
  if (!faucet) {
    throw new Error(`Missing faucet actor "alice"${selectedActorsFilePath ? ` from ${selectedActorsFilePath}` : ''}`);
  }

  for (const name of actorNames) {
    const actor = actors[name];
    if (!actor) {
      continue;
    }
    if (name === 'alice') {
      continue;
    }

    const free = await readFreeBalance(api, actor.address);
    if (free >= minimum) {
      continue;
    }

    const delta = minimum - free;
    const tx = api.tx.balances.transferKeepAlive(actor.address, delta.toString());
    const receipt = await submitTx(api, tx, faucet, `fund ${name}`);
    assertTxSuccess(
      receipt,
      `fund ${name}${selectedActorsFilePath ? ` via faucet ${faucet.address} from ${selectedActorsFilePath}` : ''}`,
    );
  }
}
