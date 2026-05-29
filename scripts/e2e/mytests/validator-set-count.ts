#!/usr/bin/env tsx
/**
 * Raise staking.validatorCount via sudo and optionally force a new era.
 * 通过 sudo 提高 staking.validatorCount，并可选触发新的 era。
 *
 * This is an operational helper for live chains where a waiting validator
 * cannot enter the active validator set because the current target count is too low.
 * 这是面向线上链的运维脚本：当等待中的验证者因 validator target 偏低而无法进入 active set 时使用。
 *
 * Usage / 用法:
 *   SUDO_URI='//Alice' node --import tsx e2e/mytests/validator-set-count.ts --dry-run --expect-validator <SS58>
 *   SUDO_URI='//Alice' node --import tsx e2e/mytests/validator-set-count.ts --count 4 --force-new-era --expect-validator <SS58>
 *
 * Environment / 环境变量:
 *   WS_URL                  — default wss://rpc.nexcommunity.net
 *   SUDO_URI                — Substrate URI (mnemonic, //Alice, 0x..., etc.)
 *   SUDO_MNEMONIC           — Alias of SUDO_URI
 *   TARGET_VALIDATOR_COUNT  — default 4
 *   EXPECT_VALIDATORS       — comma-separated validator stash addresses
 */

import type { ApiPromise } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';

import { connectApi, disconnectApi, submitTx, captureChainSnapshot } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { codecToJson, coerceNumber, readObjectField } from '../framework/codec.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

interface CliOptions {
  wsUrl: string;
  count: number;
  dryRun: boolean;
  forceNewEra: boolean;
  allowPartial: boolean;
  json: boolean;
  expectValidators: string[];
}

interface ValidatorReadiness {
  stash: string;
  isActiveNow: boolean;
  hasValidatorPrefs: boolean;
  bondedController: string | null;
  hasLedger: boolean;
  hasNextKeys: boolean | null;
  nextKeysSource: 'session.nextKeys' | 'session.queuedKeys' | 'unavailable';
  readiness: 'ready' | 'waiting-for-session-keys' | 'staking-prefs-missing' | 'bond-incomplete';
}

function parseCli(argv: string[]): CliOptions {
  let wsUrl = process.env.WS_URL?.trim() || 'wss://rpc.nexcommunity.net';
  let count = coerceNumber(process.env.TARGET_VALIDATOR_COUNT) ?? 4;
  let dryRun = false;
  let forceNewEra = false;
  let allowPartial = false;
  let json = false;
  const expectValidators = new Set<string>();

  const expectFromEnv = process.env.EXPECT_VALIDATORS?.trim();
  if (expectFromEnv) {
    for (const item of expectFromEnv.split(',')) {
      const value = item.trim();
      if (value) {
        expectValidators.add(value);
      }
    }
  }

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--dry-run') {
      dryRun = true;
    } else if (arg === '--json') {
      json = true;
    } else if (arg === '--force-new-era') {
      forceNewEra = true;
    } else if (arg === '--allow-partial') {
      allowPartial = true;
    } else if (arg === '--count' && argv[i + 1]) {
      const value = Number(argv[++i]);
      if (!Number.isFinite(value) || value < 1) {
        throw new Error('--count 须为正整数');
      }
      count = Math.floor(value);
    } else if (arg === '--ws' && argv[i + 1]) {
      wsUrl = argv[++i];
    } else if (arg === '--expect-validator' && argv[i + 1]) {
      expectValidators.add(argv[++i]);
    }
  }

  return {
    wsUrl,
    count,
    dryRun,
    forceNewEra,
    allowPartial,
    json,
    expectValidators: Array.from(expectValidators),
  };
}

async function loadSigner() {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const uri = process.env.SUDO_URI?.trim() || process.env.SUDO_MNEMONIC?.trim();
  if (!uri) {
    throw new Error('请设置 SUDO_URI 或 SUDO_MNEMONIC 作为 sudo 签名账户');
  }
  return keyring.addFromUri(uri);
}

function boolish(value: unknown): boolean {
  if (typeof value === 'boolean') {
    return value;
  }
  if (typeof value === 'string') {
    return value === 'true';
  }
  return false;
}

function hasMeaningfulKeys(record: unknown): boolean {
  if (!record || typeof record !== 'object') {
    return false;
  }
  return Object.values(record as Record<string, unknown>).some((value) => {
    if (typeof value === 'string') {
      return value.length > 0 && value !== '0x' && !/^0x0+$/.test(value);
    }
    if (Array.isArray(value)) {
      return value.length > 0;
    }
    return value != null;
  });
}

async function readValidatorCount(api: ApiPromise): Promise<number | null> {
  const stakingQuery = (api.query as any).staking;
  const stakingConst = (api.consts as any).staking;

  if (stakingQuery && typeof stakingQuery.validatorCount === 'function') {
    const value = await stakingQuery.validatorCount();
    return coerceNumber(value.toString()) ?? coerceNumber(codecToJson(value)) ?? null;
  }

  if (stakingConst?.validatorCount) {
    return coerceNumber(stakingConst.validatorCount.toString()) ?? null;
  }

  return null;
}

async function readCurrentSessionIndex(api: ApiPromise): Promise<number | null> {
  const sessionQuery = (api.query as any).session;
  if (sessionQuery && typeof sessionQuery.currentIndex === 'function') {
    const value = await sessionQuery.currentIndex();
    return coerceNumber(value.toString()) ?? coerceNumber(codecToJson(value)) ?? null;
  }
  return null;
}

async function readQueuedSessionKeys(api: ApiPromise, stash: string): Promise<boolean | null> {
  const sessionQuery = (api.query as any).session;
  if (!sessionQuery?.queuedKeys || typeof sessionQuery.queuedKeys.entries !== 'function') {
    return null;
  }

  const entries = await sessionQuery.queuedKeys.entries();
  for (const [key, value] of entries as Array<[unknown, unknown]>) {
    const keyArgs = (key as any)?.args;
    const account = Array.isArray(keyArgs) && keyArgs.length > 0 ? keyArgs[0]?.toString?.() : undefined;
    if (account === stash) {
      return hasMeaningfulKeys(codecToJson(value));
    }
  }
  return false;
}

async function checkValidatorReadiness(api: ApiPromise, stash: string, activeSet: Set<string>): Promise<ValidatorReadiness> {
  const stakingQuery = (api.query as any).staking;
  const sessionQuery = (api.query as any).session;

  const prefsCodec = stakingQuery?.validators ? await stakingQuery.validators(stash) : null;
  const prefsJson = prefsCodec ? codecToJson<Record<string, unknown>>(prefsCodec) : null;
  const commission = readObjectField(prefsJson, 'commission');
  const blocked = readObjectField(prefsJson, 'blocked');
  const hasValidatorPrefs = prefsCodec != null && (!prefsCodec.isEmpty || commission !== undefined || blocked !== undefined || Object.keys(prefsJson ?? {}).length > 0);

  const bondedCodec = stakingQuery?.bonded ? await stakingQuery.bonded(stash) : null;
  const bondedController = bondedCodec && !bondedCodec.isEmpty ? bondedCodec.toString() : null;

  let hasLedger = false;
  if (bondedController && stakingQuery?.ledger) {
    const ledgerCodec = await stakingQuery.ledger(bondedController);
    hasLedger = !ledgerCodec.isEmpty;
  }

  let hasNextKeys: boolean | null = null;
  let nextKeysSource: ValidatorReadiness['nextKeysSource'] = 'unavailable';
  if (sessionQuery?.nextKeys) {
    nextKeysSource = 'session.nextKeys';
    const nextKeysCodec = await sessionQuery.nextKeys(stash);
    if (nextKeysCodec?.isNone === true) {
      hasNextKeys = false;
    } else if (nextKeysCodec?.isSome === true) {
      hasNextKeys = hasMeaningfulKeys(codecToJson(nextKeysCodec.unwrap()));
    } else if (nextKeysCodec?.isEmpty === true) {
      hasNextKeys = false;
    } else {
      hasNextKeys = hasMeaningfulKeys(codecToJson(nextKeysCodec));
    }
  } else {
    const queuedKeys = await readQueuedSessionKeys(api, stash);
    if (queuedKeys !== null) {
      hasNextKeys = queuedKeys;
      nextKeysSource = 'session.queuedKeys';
    }
  }

  let readiness: ValidatorReadiness['readiness'];
  if (!hasValidatorPrefs) {
    readiness = 'staking-prefs-missing';
  } else if (!bondedController || !hasLedger) {
    readiness = 'bond-incomplete';
  } else if (hasNextKeys === false) {
    readiness = 'waiting-for-session-keys';
  } else {
    readiness = 'ready';
  }

  return {
    stash,
    isActiveNow: activeSet.has(stash),
    hasValidatorPrefs,
    bondedController,
    hasLedger,
    hasNextKeys,
    nextKeysSource,
    readiness,
  };
}

interface ScriptReport {
  chain: {
    chain: string;
    nodeName: string;
    nodeVersion: string;
    specName: string;
    specVersion: number;
    wsUrl: string;
  };
  preflight: {
    sudoSigner: string;
    sudoFreeBalancePlanck: string;
    validatorCountBefore: number | null;
    targetValidatorCount: number;
    activeEra: number | null;
    sessionIndex: number | null;
    activeValidatorsNow: string[];
    waitingValidators: ValidatorReadiness[];
  };
  actions: {
    dryRun: boolean;
    allowPartial: boolean;
    forceNewEra: boolean;
    needsUpdate: boolean;
    plannedCalls: string[];
    receipts: Array<{ label: string; success: boolean; txHash: string; error?: string }>;
  };
  postCheck?: {
    validatorCountAfter: number | null;
    activeEraAfter: number | null;
    activeValidatorsAfter: string[];
  };
}

function printReadiness(rows: ValidatorReadiness[]): void {
  if (rows.length === 0) {
    console.log('未提供待检查验证者地址，跳过 readiness 检查。');
    return;
  }

  console.log('\n[waiting validators readiness]');
  for (const row of rows) {
    console.log(`- stash: ${row.stash}`);
    console.log(`  active_now: ${row.isActiveNow}`);
    console.log(`  has_validator_prefs: ${row.hasValidatorPrefs}`);
    console.log(`  bonded_controller: ${row.bondedController ?? 'none'}`);
    console.log(`  has_ledger: ${row.hasLedger}`);
    console.log(`  has_next_keys: ${row.hasNextKeys == null ? 'unknown' : row.hasNextKeys}`);
    console.log(`  next_keys_source: ${row.nextKeysSource}`);
    console.log(`  readiness: ${row.readiness}`);
  }
}

function codecArrayToStrings(value: any): string[] {
  const items: unknown[] = value && typeof value.toArray === 'function'
    ? value.toArray()
    : Array.from(value as Iterable<unknown>);
  return items.map((item) => String(item));
}

async function main(): Promise<void> {
  const options = parseCli(process.argv.slice(2));
  process.env.WS_URL = options.wsUrl;

  const api = await connectApi(options.wsUrl);
  try {
    const chain = await captureChainSnapshot(api);
    const signer = await loadSigner();
    const free = await readFreeBalance(api, signer.address);

    const validatorCountBefore = await readValidatorCount(api);
    const activeEraCodec = await api.query.staking.activeEra();
    const activeEraJson = codecToJson<{ index?: unknown } | null>(activeEraCodec);
    const activeEra = coerceNumber(readObjectField(activeEraJson, 'index')) ?? null;
    const currentSessionIndex = await readCurrentSessionIndex(api);
    const sessionValidatorsCodec = await api.query.session.validators();
    const sessionValidators = codecArrayToStrings(sessionValidatorsCodec);
    const activeSet: Set<string> = new Set(sessionValidators);

    const readinessRows: ValidatorReadiness[] = [];
    for (const stash of options.expectValidators) {
      readinessRows.push(await checkValidatorReadiness(api, stash, activeSet));
    }

    const plannedCalls: string[] = [];
    const receipts: ScriptReport['actions']['receipts'] = [];

    if (!options.json) {
      console.log('[chain]');
      console.log(`- chain: ${chain.chain}`);
      console.log(`- node: ${chain.nodeName} ${chain.nodeVersion}`);
      console.log(`- spec: ${chain.specName} v${chain.specVersion}`);
      console.log(`- ws: ${options.wsUrl}`);

      console.log('\n[preflight]');
      console.log(`- sudo_signer: ${signer.address}`);
      console.log(`- sudo_free_balance_planck: ${free.toString()}`);
      console.log(`- validator_count_before: ${validatorCountBefore ?? 'unknown'}`);
      console.log(`- target_validator_count: ${options.count}`);
      console.log(`- active_era: ${activeEra ?? 'unknown'}`);
      console.log(`- session_index: ${currentSessionIndex ?? 'unknown'}`);
      console.log(`- active_validators_now: ${sessionValidators.length}`);
      for (const validator of sessionValidators) {
        console.log(`  - ${validator}`);
      }

      printReadiness(readinessRows);
    }

    const notReady = readinessRows.filter((row) => row.readiness !== 'ready' && !row.isActiveNow);
    if (notReady.length > 0 && !options.allowPartial) {
      throw new Error(`存在 ${notReady.length} 个 waiting validator 未通过 readiness 检查；如确需继续，请加 --allow-partial`);
    }

    const stakingTx = (api.tx as any).staking;
    const sudoTx = (api.tx as any).sudo;
    if (!stakingTx?.setValidatorCount) {
      throw new Error('链上 runtime 缺少 staking.setValidatorCount');
    }
    if (!sudoTx?.sudo) {
      throw new Error('链上 runtime 缺少 sudo.sudo');
    }
    if (options.forceNewEra && !stakingTx?.forceNewEra) {
      throw new Error('链上 runtime 缺少 staking.forceNewEra');
    }

    const needsUpdate = validatorCountBefore == null || validatorCountBefore !== options.count;
    if (needsUpdate) {
      plannedCalls.push(`sudo(staking.setValidatorCount(${options.count}))`);
    }
    if (options.forceNewEra) {
      plannedCalls.push('sudo(staking.forceNewEra())');
    }
    if (!needsUpdate && !options.json) {
      console.log(`\nvalidatorCount 已经是 ${options.count}，无需 setValidatorCount。`);
    }

    if (options.dryRun) {
      const report: ScriptReport = {
        chain: {
          chain: chain.chain,
          nodeName: chain.nodeName,
          nodeVersion: chain.nodeVersion,
          specName: chain.specName,
          specVersion: chain.specVersion,
          wsUrl: options.wsUrl,
        },
        preflight: {
          sudoSigner: signer.address,
          sudoFreeBalancePlanck: free.toString(),
          validatorCountBefore,
          targetValidatorCount: options.count,
          activeEra: activeEra,
          sessionIndex: currentSessionIndex,
          activeValidatorsNow: sessionValidators,
          waitingValidators: readinessRows,
        },
        actions: {
          dryRun: true,
          allowPartial: options.allowPartial,
          forceNewEra: options.forceNewEra,
          needsUpdate,
          plannedCalls,
          receipts,
        },
      };

      if (options.json) {
        console.log(JSON.stringify(report, null, 2));
      } else {
        console.log('\n[dry-run]');
        for (const call of plannedCalls) {
          console.log(`- would ${call}`);
        }
      }
      return;
    }

    if (free === 0n) {
      throw new Error('sudo 签名账户可用余额为 0，无法支付交易手续费');
    }

    if (needsUpdate) {
      const tx = sudoTx.sudo(stakingTx.setValidatorCount(options.count));
      const receipt = await submitTx(api, tx, signer, `set validator count to ${options.count}`);
      receipts.push({
        label: `set validator count to ${options.count}`,
        success: receipt.success,
        txHash: receipt.txHash,
        error: receipt.error,
      });
      if (!receipt.success) {
        throw new Error(`setValidatorCount 失败: ${receipt.error ?? '未知错误'}`);
      }
      if (!options.json) {
        console.log(`\n成功 setValidatorCount(${options.count}) txHash=${receipt.txHash}`);
      }
    }

    if (options.forceNewEra) {
      const tx = sudoTx.sudo(stakingTx.forceNewEra());
      const receipt = await submitTx(api, tx, signer, 'force new era');
      receipts.push({
        label: 'force new era',
        success: receipt.success,
        txHash: receipt.txHash,
        error: receipt.error,
      });
      if (!receipt.success) {
        throw new Error(`forceNewEra 失败: ${receipt.error ?? '未知错误'}`);
      }
      if (!options.json) {
        console.log(`成功 forceNewEra() txHash=${receipt.txHash}`);
      }
    }

    const validatorCountAfter = await readValidatorCount(api);
    const activeEraAfterCodec = await api.query.staking.activeEra();
    const activeEraAfterJson = codecToJson<{ index?: unknown } | null>(activeEraAfterCodec);
    const activeEraAfter = coerceNumber(readObjectField(activeEraAfterJson, 'index')) ?? null;
    const sessionValidatorsAfter = codecArrayToStrings(await api.query.session.validators());

    const report: ScriptReport = {
      chain: {
        chain: chain.chain,
        nodeName: chain.nodeName,
        nodeVersion: chain.nodeVersion,
        specName: chain.specName,
        specVersion: chain.specVersion,
        wsUrl: options.wsUrl,
      },
      preflight: {
        sudoSigner: signer.address,
        sudoFreeBalancePlanck: free.toString(),
        validatorCountBefore,
        targetValidatorCount: options.count,
        activeEra,
        sessionIndex: currentSessionIndex,
        activeValidatorsNow: sessionValidators,
        waitingValidators: readinessRows,
      },
      actions: {
        dryRun: false,
        allowPartial: options.allowPartial,
        forceNewEra: options.forceNewEra,
        needsUpdate,
        plannedCalls,
        receipts,
      },
      postCheck: {
        validatorCountAfter,
        activeEraAfter,
        activeValidatorsAfter: sessionValidatorsAfter,
      },
    };

    if (options.json) {
      console.log(JSON.stringify(report, null, 2));
      return;
    }

    console.log('\n[post-check]');
    console.log(`- validator_count_after: ${validatorCountAfter ?? 'unknown'}`);
    console.log(`- active_era_after: ${activeEraAfter ?? 'unknown'}`);
    console.log(`- active_validators_after: ${sessionValidatorsAfter.length}`);
    for (const validator of sessionValidatorsAfter) {
      console.log(`  - ${validator}`);
    }
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
