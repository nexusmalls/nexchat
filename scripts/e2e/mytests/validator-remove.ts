#!/usr/bin/env tsx
/**
 * Remove a validator via sudo by optionally removing it from invulnerables,
 * force un-staking the stash, and optionally forcing a new era.
 *
 * Usage:
 *   SUDO_URI='//Alice' node --import tsx e2e/mytests/validator-remove.ts --stash <SS58> --dry-run
 *   SUDO_URI='//Alice' node --import tsx e2e/mytests/validator-remove.ts --stash <SS58> --spans 0 --force-new-era
 *
 * Environment:
 *   WS_URL         — default wss://rpc.nexcommunity.net
 *   SUDO_URI       — Substrate URI (mnemonic, //Alice, 0x..., etc.)
 *   SUDO_MNEMONIC  — alias of SUDO_URI
 */

import type { ApiPromise } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';

import { connectApi, disconnectApi, submitTx, captureChainSnapshot } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { codecToJson, coerceNumber } from '../framework/codec.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

interface CliOptions {
  wsUrl: string;
  stash: string;
  spans: number;
  dryRun: boolean;
  forceNewEra: boolean;
  keepInvulnerable: boolean;
  json: boolean;
}

interface ValidatorState {
  stash: string;
  isInvulnerable: boolean;
  invulnerablesBefore: string[];
  inSessionValidators: boolean;
  sessionValidators: string[];
  bondedController: string | null;
  hasLedger: boolean;
  ledgerJson: unknown | null;
  validatorPrefs: unknown | null;
  slashingSpansOnChain: number | null;
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
    validator: ValidatorState;
  };
  actions: {
    dryRun: boolean;
    forceNewEra: boolean;
    keepInvulnerable: boolean;
    spans: number;
    plannedCalls: string[];
    receipts: Array<{ label: string; success: boolean; txHash: string; error?: string }>;
  };
  postCheck?: {
    isInvulnerableAfter: boolean;
    invulnerablesAfter: string[];
    inSessionValidatorsAfter: boolean;
    bondedAfter: string | null;
    hasLedgerAfter: boolean;
    activeEraAfter: number | null;
  };
}

function parseCli(argv: string[]): CliOptions {
  let wsUrl = process.env.WS_URL?.trim() || 'wss://rpc.nexcommunity.net';
  let stash = '';
  let spans = 0;
  let dryRun = false;
  let forceNewEra = false;
  let keepInvulnerable = false;
  let json = false;

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--stash' && argv[i + 1]) {
      stash = argv[++i];
    } else if (arg === '--spans' && argv[i + 1]) {
      const value = Number(argv[++i]);
      if (!Number.isFinite(value) || value < 0) {
        throw new Error('--spans 须为非负整数');
      }
      spans = Math.floor(value);
    } else if (arg === '--dry-run') {
      dryRun = true;
    } else if (arg === '--force-new-era') {
      forceNewEra = true;
    } else if (arg === '--keep-invulnerable') {
      keepInvulnerable = true;
    } else if (arg === '--json') {
      json = true;
    } else if (arg === '--ws' && argv[i + 1]) {
      wsUrl = argv[++i];
    }
  }

  if (!stash) {
    throw new Error('请通过 --stash 提供要删除的 validator stash 地址');
  }

  return { wsUrl, stash, spans, dryRun, forceNewEra, keepInvulnerable, json };
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

async function readActiveEra(api: ApiPromise): Promise<number | null> {
  const activeEraCodec = await (api.query as any).staking?.activeEra?.();
  if (!activeEraCodec) {
    return null;
  }
  const activeEraJson = codecToJson<{ index?: unknown } | null>(activeEraCodec);
  return coerceNumber(activeEraJson?.index) ?? null;
}

async function readValidatorState(api: ApiPromise, stash: string): Promise<ValidatorState> {
  const stakingQuery = (api.query as any).staking;
  const sessionQuery = (api.query as any).session;

  const invulnerablesCodec = stakingQuery?.invulnerables ? await stakingQuery.invulnerables() : [];
  const invulnerablesBefore = Array.from(
    typeof invulnerablesCodec?.toArray === 'function' ? invulnerablesCodec.toArray() : invulnerablesCodec,
  ).map((item) => String(item));

  const sessionValidatorsCodec = sessionQuery?.validators ? await sessionQuery.validators() : [];
  const sessionValidators = Array.from(
    typeof sessionValidatorsCodec?.toArray === 'function' ? sessionValidatorsCodec.toArray() : sessionValidatorsCodec,
  ).map((item) => String(item));

  const bondedCodec = stakingQuery?.bonded ? await stakingQuery.bonded(stash) : null;
  const bondedController = bondedCodec && !bondedCodec.isEmpty ? bondedCodec.toString() : null;

  let hasLedger = false;
  let ledgerJson: unknown | null = null;
  if (bondedController && stakingQuery?.ledger) {
    const ledgerCodec = await stakingQuery.ledger(bondedController);
    hasLedger = !ledgerCodec.isEmpty;
    ledgerJson = hasLedger ? codecToJson(ledgerCodec) : null;
  }

  const validatorPrefsCodec = stakingQuery?.validators ? await stakingQuery.validators(stash) : null;
  const validatorPrefs = validatorPrefsCodec && !validatorPrefsCodec.isEmpty ? codecToJson(validatorPrefsCodec) : null;

  let slashingSpansOnChain: number | null = null;
  if (stakingQuery?.slashingSpans) {
    const spansCodec = await stakingQuery.slashingSpans(stash);
    if (spansCodec && !spansCodec.isNone && !spansCodec.isEmpty) {
      const unwrapped = typeof spansCodec.unwrap === 'function' ? spansCodec.unwrap() : spansCodec;
      const json = codecToJson<Record<string, unknown>>(unwrapped);
      slashingSpansOnChain = coerceNumber((json as any)?.spanIndex) ?? coerceNumber((json as any)?.span_index) ?? 0;
    }
  }

  return {
    stash,
    isInvulnerable: invulnerablesBefore.includes(stash),
    invulnerablesBefore,
    inSessionValidators: sessionValidators.includes(stash),
    sessionValidators,
    bondedController,
    hasLedger,
    ledgerJson,
    validatorPrefs,
    slashingSpansOnChain,
  };
}

function printState(state: ValidatorState): void {
  console.log('[validator preflight]');
  console.log(`- stash: ${state.stash}`);
  console.log(`- is_invulnerable: ${state.isInvulnerable}`);
  console.log(`- in_session_validators: ${state.inSessionValidators}`);
  console.log(`- bonded_controller: ${state.bondedController ?? 'none'}`);
  console.log(`- has_ledger: ${state.hasLedger}`);
  console.log(`- slashing_spans_on_chain: ${state.slashingSpansOnChain ?? 'unknown'}`);
  console.log(`- invulnerables_before: ${state.invulnerablesBefore.length}`);
  if (state.invulnerablesBefore.length > 0) {
    for (const item of state.invulnerablesBefore) {
      console.log(`  - ${item}`);
    }
  }
}

async function main(): Promise<void> {
  const options = parseCli(process.argv.slice(2));
  process.env.WS_URL = options.wsUrl;

  const api = await connectApi(options.wsUrl);
  try {
    const chain = await captureChainSnapshot(api);
    const signer = await loadSigner();
    const free = await readFreeBalance(api, signer.address);
    const stateBefore = await readValidatorState(api, options.stash);
    const activeEraBefore = await readActiveEra(api);

    const stakingTx = (api.tx as any).staking;
    const sudoTx = (api.tx as any).sudo;
    if (!sudoTx?.sudo) {
      throw new Error('链上 runtime 缺少 sudo.sudo');
    }
    if (!stakingTx?.forceUnstake) {
      throw new Error('链上 runtime 缺少 staking.forceUnstake');
    }
    if (!options.keepInvulnerable && stateBefore.isInvulnerable && !stakingTx?.setInvulnerables) {
      throw new Error('链上 runtime 缺少 staking.setInvulnerables');
    }
    if (options.forceNewEra && !stakingTx?.forceNewEra) {
      throw new Error('链上 runtime 缺少 staking.forceNewEra');
    }

    const plannedCalls: string[] = [];
    const receipts: ScriptReport['actions']['receipts'] = [];

    if (!options.keepInvulnerable && stateBefore.isInvulnerable) {
      const nextInvulnerables = stateBefore.invulnerablesBefore.filter((item) => item !== options.stash);
      plannedCalls.push(`sudo(staking.setInvulnerables([${nextInvulnerables.join(', ')}]))`);
    }
    plannedCalls.push(`sudo(staking.forceUnstake(${options.stash}, ${options.spans}))`);
    if (options.forceNewEra) {
      plannedCalls.push('sudo(staking.forceNewEra())');
    }

    if (!options.json) {
      console.log('[chain]');
      console.log(`- chain: ${chain.chain}`);
      console.log(`- node: ${chain.nodeName} ${chain.nodeVersion}`);
      console.log(`- spec: ${chain.specName} v${chain.specVersion}`);
      console.log(`- ws: ${options.wsUrl}`);
      console.log('\n[preflight]');
      console.log(`- sudo_signer: ${signer.address}`);
      console.log(`- sudo_free_balance_planck: ${free.toString()}`);
      console.log(`- active_era_before: ${activeEraBefore ?? 'unknown'}`);
      printState(stateBefore);
      console.log('\n[plan]');
      console.log(`- keep_invulnerable: ${options.keepInvulnerable}`);
      console.log(`- force_new_era: ${options.forceNewEra}`);
      console.log(`- spans: ${options.spans}`);
      for (const call of plannedCalls) {
        console.log(`  - ${call}`);
      }
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
          validator: stateBefore,
        },
        actions: {
          dryRun: true,
          forceNewEra: options.forceNewEra,
          keepInvulnerable: options.keepInvulnerable,
          spans: options.spans,
          plannedCalls,
          receipts,
        },
      };
      if (options.json) {
        console.log(JSON.stringify(report, null, 2));
      }
      return;
    }

    if (!options.keepInvulnerable && stateBefore.isInvulnerable) {
      const nextInvulnerables = stateBefore.invulnerablesBefore.filter((item) => item !== options.stash);
      const tx = sudoTx.sudo(stakingTx.setInvulnerables(nextInvulnerables));
      const receipt = await submitTx(api, tx, signer, 'remove invulnerable');
      receipts.push({ label: receipt.label, success: receipt.success, txHash: receipt.txHash, error: receipt.error });
      if (!receipt.success) {
        throw new Error(`remove invulnerable 失败: ${receipt.error ?? '未知错误'}`);
      }
      if (!options.json) {
        console.log(`成功 remove invulnerable txHash=${receipt.txHash}`);
      }
    }

    {
      const tx = sudoTx.sudo(stakingTx.forceUnstake(options.stash, options.spans));
      const receipt = await submitTx(api, tx, signer, 'force unstake validator');
      receipts.push({ label: receipt.label, success: receipt.success, txHash: receipt.txHash, error: receipt.error });
      if (!receipt.success) {
        throw new Error(`forceUnstake 失败: ${receipt.error ?? '未知错误'}`);
      }
      if (!options.json) {
        console.log(`成功 forceUnstake(${options.stash}, ${options.spans}) txHash=${receipt.txHash}`);
      }
    }

    if (options.forceNewEra) {
      const tx = sudoTx.sudo(stakingTx.forceNewEra());
      const receipt = await submitTx(api, tx, signer, 'force new era');
      receipts.push({ label: receipt.label, success: receipt.success, txHash: receipt.txHash, error: receipt.error });
      if (!receipt.success) {
        throw new Error(`forceNewEra 失败: ${receipt.error ?? '未知错误'}`);
      }
      if (!options.json) {
        console.log(`成功 forceNewEra() txHash=${receipt.txHash}`);
      }
    }

    const stateAfter = await readValidatorState(api, options.stash);
    const activeEraAfter = await readActiveEra(api);

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
        validator: stateBefore,
      },
      actions: {
        dryRun: false,
        forceNewEra: options.forceNewEra,
        keepInvulnerable: options.keepInvulnerable,
        spans: options.spans,
        plannedCalls,
        receipts,
      },
      postCheck: {
        isInvulnerableAfter: stateAfter.isInvulnerable,
        invulnerablesAfter: stateAfter.invulnerablesBefore,
        inSessionValidatorsAfter: stateAfter.inSessionValidators,
        bondedAfter: stateAfter.bondedController,
        hasLedgerAfter: stateAfter.hasLedger,
        activeEraAfter,
      },
    };

    if (options.json) {
      console.log(JSON.stringify(report, null, 2));
      return;
    }

    console.log('\n[post-check]');
    console.log(`- is_invulnerable_after: ${stateAfter.isInvulnerable}`);
    console.log(`- in_session_validators_after: ${stateAfter.inSessionValidators}`);
    console.log(`- bonded_after: ${stateAfter.bondedController ?? 'none'}`);
    console.log(`- has_ledger_after: ${stateAfter.hasLedger}`);
    console.log(`- active_era_after: ${activeEraAfter ?? 'unknown'}`);
  } finally {
    await disconnectApi(api);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
