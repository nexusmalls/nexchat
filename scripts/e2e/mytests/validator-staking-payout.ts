#!/usr/bin/env tsx
/**
 * Claim validator staking era rewards via `staking.payoutStakersByPage` (pallet-staking ~SDK 45).
 * 对未领取的 era / page 自动发 payout（任意账户可代付手续费）。
 *
 * Uses the same unclaimed detection as `validator-staking-audit.ts` (see `staking-unclaimed.ts`).
 *
 * Usage / 用法:
 *   PAYOUT_URI='//Alice' node --import tsx e2e/mytests/validator-staking-payout.ts --dry-run
 *   PAYOUT_MNEMONIC='twelve words...' node --import tsx e2e/mytests/validator-staking-payout.ts --stash <SS58>
 *
 * Flags / 参数:
 *   --dry-run     Print planned extrinsics only (no signature).
 *   --yes         Skip interactive confirmation prompts and submit automatically.
 *   --stash SS58  Only this validator stash (default: all session validators).
 *   --max N       Max successful payouts to submit (default 200).
 *   --insecure    Disable TLS certificate verification for the WSS connection.
 *   --max-validators N  Only scan the first N validators after filtering.
 *
 * Environment / 环境变量:
 *   WS_URL           — default wss://rpc.nexusmall.net
 *   PAYOUT_URI       — Substrate URI (mnemonic phrase, `//Alice`, `0x...`, etc.)
 *   PAYOUT_MNEMONIC  — Alias: treated as URI (mnemonic only)
 *   PAYOUT_MAX_VALIDATORS — default scan limit when `--max-validators` is omitted
 */

process.env.WS_URL ??= 'wss://rpc.nexusmall.net';

import { createInterface } from 'node:readline/promises';
import { stdin as input, stdout as output } from 'node:process';

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';

import { connectApi, disconnectApi, submitTx } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { codecToJson, coerceNumber } from '../framework/codec.js';
import { parseStakingRewardedEventData } from '../framework/staking-rewarded.js';
import {
  buildPayoutEraRange,
  getStakingEraPagePreview,
  listUnclaimedEraPages,
  type StakingEraPagePreview,
} from '../framework/staking-unclaimed.js';
import { formatNex } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

interface CliOptions {
  dryRun: boolean;
  yes: boolean;
  insecure: boolean;
  stashFilter: string | undefined;
  maxTxs: number;
  maxValidators: number | undefined;
}

function parseCli(argv: string[]): CliOptions {
  let dryRun = false;
  let yes = false;
  let insecure = false;
  let stashFilter: string | undefined;
  let maxTxs = coerceNumber(process.env.PAYOUT_MAX_TXS) ?? 200;
  let maxValidators = coerceNumber(process.env.PAYOUT_MAX_VALIDATORS);

  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--dry-run') {
      dryRun = true;
    } else if (a === '--yes') {
      yes = true;
    } else if (a === '--insecure') {
      insecure = true;
    } else if (a === '--stash' && argv[i + 1]) {
      stashFilter = argv[++i];
    } else if (a === '--max' && argv[i + 1]) {
      const n = Number(argv[++i]);
      if (!Number.isFinite(n) || n < 1) {
        throw new Error('--max 须为正整数');
      }
      maxTxs = Math.floor(n);
    } else if (a === '--max-validators' && argv[i + 1]) {
      const n = Number(argv[++i]);
      if (!Number.isFinite(n) || n < 1) {
        throw new Error('--max-validators 须为正整数');
      }
      maxValidators = Math.floor(n);
    }
  }

  if (dryRun && yes) {
    throw new Error('--dry-run 与 --yes 无需同时使用');
  }

  if (maxValidators != null && maxValidators < 1) {
    throw new Error('--max-validators 须为正整数');
  }

  return { dryRun, yes, insecure, stashFilter, maxTxs, maxValidators };
}

async function loadSigner(): Promise<ReturnType<Keyring['addFromUri']>> {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const uri = process.env.PAYOUT_URI?.trim() || process.env.PAYOUT_MNEMONIC?.trim();
  if (!uri) {
    throw new Error(
      '请设置 PAYOUT_URI 或 PAYOUT_MNEMONIC 作为签名账户（支付手续费）。示例: PAYOUT_URI="//Alice"',
    );
  }
  return keyring.addFromUri(uri);
}

interface PayoutOp {
  stash: string;
  era: number;
  page: number;
  path: 'paged' | 'legacy';
}

interface PayoutSummaryTx {
  stash: string;
  era: number;
  page: number;
  txHash: string;
  totalRewardedPlanck: string;
  rewardedEventCount: number;
}

interface PayoutSummary {
  type: 'staking-payout-summary';
  activeEra: number;
  historyDepth: number;
  plannedOps: number;
  maxTxs: number;
  dryRun: boolean;
  autoConfirmed: boolean;
  signer: string | null;
  submitted: number;
  successful: number;
  failed: number;
  totalRewardedPlanck: string;
  txs: PayoutSummaryTx[];
}

/**
 * 从本笔 extrinsic 回执里汇总质押奖励事件金额（一页内可能多条：验证人 + 提名者）。
 */
function summarizeStakingRewarded(
  events: Array<{ section: string; method: string; data: unknown }>,
): {
  totalPlanck: bigint;
  details: Array<{ stash: string; amountPlanck: bigint }>;
} {
  const details: Array<{ stash: string; amountPlanck: bigint }> = [];
  for (const ev of events) {
    if (ev.section !== 'staking' || ev.method !== 'Rewarded') {
      continue;
    }
    const parsed = parseStakingRewardedEventData(ev.data);
    if (!parsed || !parsed.stash) {
      continue;
    }
    details.push({ stash: parsed.stash, amountPlanck: parsed.amountPlanck });
  }
  const totalPlanck = details.reduce((s, d) => s + d.amountPlanck, 0n);
  return { totalPlanck, details };
}

function buildOps(stash: string, unclaimed: Awaited<ReturnType<typeof listUnclaimedEraPages>>): PayoutOp[] {
  const ops: PayoutOp[] = [];
  for (const row of unclaimed) {
    for (const page of row.missingPages) {
      ops.push({ stash, era: row.era, page, path: row.path });
    }
  }
  ops.sort((a, b) => (a.stash === b.stash ? (a.era === b.era ? a.page - b.page : a.era - b.era) : a.stash.localeCompare(b.stash)));
  return ops;
}

function renderPreviewText(preview: StakingEraPagePreview): void {
  console.log('');
  console.log(`  预览: stash=${preview.stash} | era=${preview.era} | page=${preview.page}`);
  console.log(
    `    路径=${preview.path === 'paged' ? '分页' : '旧版'} | 已领取=${preview.isClaimed ? '是' : '否'} | payout窗口内=${preview.withinPayoutWindow ? '是' : '否'}`,
  );
  console.log(
    `    总曝光=${formatNex(BigInt(preview.exposureTotalPlanck))} | 本页曝光=${formatNex(BigInt(preview.pageExposureTotalPlanck))} | 验证人自押=${formatNex(BigInt(preview.validatorOwnStakePlanck))}`,
  );
  if (!preview.computable) {
    console.log(`    预计奖励: 暂不可计算${preview.reason ? `（${preview.reason}）` : ''}`);
    return;
  }
  console.log(
    `    预计 validator 奖励=${formatNex(BigInt(preview.recipients.find((r) => r.role === 'validator')?.estimatedRewardPlanck ?? '0'))}`,
  );
  for (const row of preview.recipients.filter((r) => r.role === 'nominator')) {
    console.log(
      `    预计 nominator ${row.stash} | stake=${formatNex(BigInt(row.stakePlanck))} | reward=${formatNex(BigInt(row.estimatedRewardPlanck ?? '0'))}`,
    );
  }
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number, label: string): Promise<T> {
  return await new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`${label} 超时（>${timeoutMs}ms）`)), timeoutMs);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

async function confirmOrAbort(prompt: string): Promise<void> {
  const rl = createInterface({ input, output });
  try {
    const answer = (await rl.question(`${prompt} 输入 y/yes 继续，其它任意输入取消: `)).trim().toLowerCase();
    if (answer !== 'y' && answer !== 'yes') {
      throw new Error('用户已取消操作。');
    }
  } finally {
    rl.close();
  }
}

async function main(): Promise<void> {
  const { dryRun, yes, insecure, stashFilter, maxTxs, maxValidators } = parseCli(process.argv.slice(2));
  if (insecure) {
    process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';
  }

  const connectTimeoutMs = coerceNumber(process.env.PAYOUT_CONNECT_TIMEOUT_MS) ?? 30_000;
  console.log(`正在连接 ${process.env.WS_URL} ...`);
  const api = await withTimeout(connectApi(process.env.WS_URL), connectTimeoutMs, '连接链节点');
  console.log('已连接，正在查询当前 era 与验证者列表...');

  try {
    const activeEraCodec = await api.query.staking.activeEra();
    const activeEraIndex = coerceNumber((codecToJson(activeEraCodec) as { index: number }).index) ?? 0;
    const historyDepth = api.consts.staking.historyDepth.toNumber();
    const payoutEras = buildPayoutEraRange(activeEraIndex, historyDepth);

    const sessionVals = await api.query.session.validators();
    let stashes = sessionVals.toArray().map((a) => a.toString());
    const totalValidators = stashes.length;
    if (stashFilter) {
      stashes = stashes.filter((s) => s === stashFilter);
      if (stashes.length === 0) {
        stashes = [stashFilter];
      }
    }
    if (!stashFilter && maxValidators != null) {
      stashes = stashes.slice(0, maxValidators);
    }
    console.log(
      `当前 era: ${activeEraIndex} | 可检查领取的 era 窗口长度: ${payoutEras.length} | 待扫描验证者: ${stashes.length}${stashFilter ? `（指定 stash）` : ` / 全部 ${totalValidators}${maxValidators != null ? `，受 --max-validators=${maxValidators} 限制` : ''}`}`,
    );

    const stakingExtrinsic = api.tx.staking as any;
    if (typeof stakingExtrinsic.payoutStakersByPage !== 'function') {
      throw new Error('链上 runtime 缺少 staking.payoutStakersByPage，请升级 @polkadot/api 或核对链元数据。');
    }

    const allOps: PayoutOp[] = [];
    const ledgerByStash = new Map<string, Record<string, unknown> | null>();

    for (let index = 0; index < stashes.length; index++) {
      const stash = stashes[index];
      console.log(`扫描进度 ${index + 1}/${stashes.length}: ${stash}`);
      const bonded = await api.query.staking.bonded(stash);
      const controller = bonded.isEmpty ? null : bonded.toString();
      let ledgerJson: Record<string, unknown> | null = null;
      if (controller) {
        const ledger = await api.query.staking.ledger(controller);
        if (!ledger.isEmpty) {
          ledgerJson = codecToJson(ledger) as Record<string, unknown>;
        }
      }
      ledgerByStash.set(stash, ledgerJson);
      const unclaimed = await listUnclaimedEraPages(api, stash, ledgerJson, payoutEras);
      const ops = buildOps(stash, unclaimed);
      allOps.push(...ops);
      console.log(`  未领取页 ${ops.length} 笔`);
    }

    const summary: PayoutSummary = {
      type: 'staking-payout-summary',
      activeEra: activeEraIndex,
      historyDepth,
      plannedOps: allOps.length,
      maxTxs,
      dryRun,
      autoConfirmed: yes,
      signer: null,
      submitted: 0,
      successful: 0,
      failed: 0,
      totalRewardedPlanck: '0',
      txs: [],
    };

    console.log(
      `已完成扫描，计划链上操作数: ${allOps.length} | 领取上限: ${maxTxs}`,
    );
    if (allOps.length === 0) {
      console.log('无需领取：窗口内没有未领页。');
      console.log(JSON.stringify(summary));
      return;
    }

    for (const op of allOps.slice(0, 20)) {
      console.log(
        `  计划: 调用领取方法 payoutStakersByPage | 验证人资金账户=${op.stash} | era=${op.era} | 页=${op.page} | ${op.path === 'paged' ? '分页' : '旧版'}`,
      );
    }
    if (allOps.length > 20) {
      console.log(`  … 另有 ${allOps.length - 20} 条`);
    }

    if (dryRun) {
      console.log('试运行结束：未提交任何交易。');
      console.log(JSON.stringify(summary));
      return;
    }

    if (!yes) {
      await confirmOrAbort(`即将进入真实 payout 模式，共 ${Math.min(allOps.length, maxTxs)} 笔待提交。`);
    } else {
      console.log(`已启用 --yes，自动进入真实 payout 模式（最多提交 ${Math.min(allOps.length, maxTxs)} 笔）。`);
    }

    const signer = await loadSigner();
    summary.signer = signer.address;
    const free = await readFreeBalance(api, signer.address);
    console.log(`签名账户: ${signer.address} | 可用余额: ${formatNex(free)}`);
    if (free === 0n) {
      throw new Error('签名账户可用余额为 0，无法支付交易手续费。');
    }

    let submitted = 0;
    let failed = 0;
    let totalRewardedPlanck = 0n;
    for (const op of allOps) {
      if (submitted >= maxTxs) {
        console.log(`已达 --max ${maxTxs} 笔成功上链，停止继续提交。`);
        break;
      }

      const preview = await getStakingEraPagePreview(
        api,
        op.stash,
        ledgerByStash.get(op.stash) ?? null,
        op.era,
        op.page,
        payoutEras,
      );
      renderPreviewText(preview);
      if (!yes) {
        await confirmOrAbort(`确认提交 ${op.stash} era=${op.era} page=${op.page} 的 payout 吗？`);
      }

      const tx = stakingExtrinsic.payoutStakersByPage(op.stash, op.era, op.page);
      const label = `领取 ${op.stash} era=${op.era} page=${op.page}`;
      const receipt = await submitTx(api, tx, signer, label);

      if (!receipt.success) {
        failed++;
        console.error(`失败 ${label}: ${receipt.error ?? '未知错误'}`);
        continue;
      }

      submitted++;
      const { totalPlanck, details } = summarizeStakingRewarded(receipt.events);
      totalRewardedPlanck += totalPlanck;
      summary.txs.push({
        stash: op.stash,
        era: op.era,
        page: op.page,
        txHash: receipt.txHash,
        totalRewardedPlanck: totalPlanck.toString(),
        rewardedEventCount: details.length,
      });
      if (details.length === 0) {
        console.log(`成功 ${label} 交易哈希=${receipt.txHash} | 本笔未解析到质押奖励事件（可在区块浏览器核对）`);
      } else {
        const parts = details.map((d) => `${d.stash} +${formatNex(d.amountPlanck)}`);
        const detailStr = details.length > 1 ? ` | 分项: ${parts.join('；')}` : '';
        console.log(
          `成功 ${label} 交易哈希=${receipt.txHash} | 本笔发放合计 ${formatNex(totalPlanck)}（${details.length} 条奖励事件）${detailStr}`,
        );
      }
    }

    summary.submitted = submitted + failed;
    summary.successful = submitted;
    summary.failed = failed;
    summary.totalRewardedPlanck = totalRewardedPlanck.toString();

    console.log(
      `完成。成功领取 ${submitted} 笔（计划最多 ${Math.min(allOps.length, maxTxs)} 笔，受 --max 限制）。`,
    );
    console.log(JSON.stringify(summary));
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
