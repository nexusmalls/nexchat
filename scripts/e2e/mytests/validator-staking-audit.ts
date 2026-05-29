#!/usr/bin/env tsx
/**
 * Validator staking snapshot + unclaimed era pages + Rewarded event rollup.
 * 验证人质押快照、未领取 era（按分页）、以及 Rewarded 事件汇总。
 *
 * Phase A / 阶段 A: session 验证人集合、bond、active era 下曝光、佣金偏好。
 * Phase B / 阶段 B: 在 historyDepth 窗口内，列出未完全领取的 era 及缺失的 reward page。
 * Events / 事件: 扫描最近 N 个已终结区块中的 staking.Rewarded，按 stash 汇总金额与笔数。
 *
 * Usage / 用法:
 *   node --import tsx e2e/mytests/validator-staking-audit.ts
 *   node --import tsx e2e/mytests/validator-staking-audit.ts --stash <SS58>
 *   node --import tsx e2e/mytests/validator-staking-audit.ts --json
 *   node --import tsx e2e/mytests/validator-staking-audit.ts --event-blocks 3000
 *   node --import tsx e2e/mytests/validator-staking-audit.ts --event-blocks 0
 *
 * Environment / 环境变量:
 *   WS_URL              — default wss://rpc.nexcommunity.net
 *   NODE_TLS_REJECT_UNAUTHORIZED — set to 0 if TLS to public RPC fails (same as other remote scripts)
 *   EVENT_SCAN_PROGRESS_EVERY — 事件扫描时每 N 块打印一行进度（默认 100）
 */

process.env.WS_URL ??= 'wss://rpc.nexcommunity.net';
process.env.NODE_TLS_REJECT_UNAUTHORIZED ??= '0';

import type { ApiPromise } from '@polkadot/api';
import type { EventRecord } from '@polkadot/types/interfaces';

import { connectApi, disconnectApi, captureChainSnapshot } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { parseStakingRewardedEventData } from '../framework/staking-rewarded.js';
import {
  bigFromHexOrDec,
  buildPayoutEraRange,
  getStakingEraPagePreview,
  listUnclaimedEraPages,
  type StakingEraPagePreview,
  type UnclaimedEraRow,
} from '../framework/staking-unclaimed.js';
import { formatNex } from '../framework/units.js';

/* -------------------------------------------------------------------------- */
/*  CLI                                                                        */
/* -------------------------------------------------------------------------- */

interface CliOptions {
  stashFilter: string | undefined;
  json: boolean;
  eventBlocks: number;
  previewEra: number | undefined;
  previewPage: number | undefined;
}

function parseCli(argv: string[]): CliOptions {
  let stashFilter: string | undefined;
  let json = false;
  let eventBlocks = coerceNumber(process.env.EVENT_BLOCKS) ?? 3000;
  let previewEra: number | undefined;
  let previewPage: number | undefined;

  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--json') {
      json = true;
    } else if (a === '--stash' && argv[i + 1]) {
      stashFilter = argv[++i];
    } else if (a === '--event-blocks' && argv[i + 1]) {
      const n = Number(argv[++i]);
      if (!Number.isFinite(n) || n < 0) {
        throw new Error('--event-blocks 须为非负整数');
      }
      eventBlocks = Math.floor(n);
    } else if (a === '--preview-era' && argv[i + 1]) {
      const n = Number(argv[++i]);
      if (!Number.isFinite(n) || n < 0) {
        throw new Error('--preview-era 须为非负整数');
      }
      previewEra = Math.floor(n);
    } else if (a === '--preview-page' && argv[i + 1]) {
      const n = Number(argv[++i]);
      if (!Number.isFinite(n) || n < 0) {
        throw new Error('--preview-page 须为非负整数');
      }
      previewPage = Math.floor(n);
    }
  }

  const wantsPreview = previewEra != null || previewPage != null;
  if (wantsPreview && (previewEra == null || previewPage == null)) {
    throw new Error('预览模式需同时提供 --preview-era 与 --preview-page');
  }
  if (wantsPreview && !stashFilter) {
    throw new Error('预览模式需通过 --stash 指定单个验证人');
  }

  return { stashFilter, json, eventBlocks, previewEra, previewPage };
}

/* -------------------------------------------------------------------------- */
/*  Event scan                                                                 */
/* -------------------------------------------------------------------------- */

interface RewardedSummary {
  lookbackBlocks: number;
  range: { from: number; to: number };
  /** 实际成功读取 events 的块区间（从链尖向下扫，遇修剪则变短） */
  scannedRange: { from: number; to: number } | null;
  blocksScanned: number;
  /** 因状态已丢弃 / 未知块等提前结束扫描 */
  stoppedEarlyPruned: boolean;
  eventCount: number;
  eventCountByStash: Record<string, number>;
  amountByStashPlanck: Record<string, string>;
  recentSamplesByStash: Record<string, Array<{ block: number; amountPlanck: string }>>;
  samples: Array<{ block: number; stash: string; amountPlanck: string }>;
}

function isStatePrunedRpcError(message: string): boolean {
  return /discarded|unknown block|State already|4003|not found|prun/i.test(message);
}

interface ScanRewardedOptions {
  /** 为 false 时不打印进度（例如 --json） */
  showProgress?: boolean;
}

function renderPreviewText(preview: StakingEraPagePreview): void {
  console.log('\n════════════════════════════════════════');
  console.log(`奖励预览: stash=${preview.stash} | era=${preview.era} | page=${preview.page}`);
  console.log(
    `路径: ${preview.path === 'paged' ? '分页' : '旧版'} | payout 窗口内: ${preview.withinPayoutWindow ? '是' : '否'} | 已领取: ${preview.isClaimed ? '是' : '否'}`,
  );
  console.log(`页数: ${preview.pageCount} | 已领页: ${JSON.stringify(preview.claimedPages)}`);
  console.log(
    `总曝光: ${formatNex(BigInt(preview.exposureTotalPlanck))} | 本页曝光: ${formatNex(BigInt(preview.pageExposureTotalPlanck))} | 验证人自押: ${formatNex(BigInt(preview.validatorOwnStakePlanck))}`,
  );
  if (preview.validatorCommissionPerbill != null) {
    console.log(`验证人佣金(perbill): ${preview.validatorCommissionPerbill}`);
  }
  if (!preview.computable) {
    console.log(`预计奖励: 暂不可计算${preview.reason ? `（${preview.reason}）` : ''}`);
  } else {
    console.log(
      `预计验证人 era 奖励: ${formatNex(BigInt(preview.estimatedTotalRewardPlanck ?? '0'))} | 佣金部分: ${formatNex(BigInt(preview.estimatedCommissionRewardPlanck ?? '0'))} | 按质押分配部分: ${formatNex(BigInt(preview.estimatedSharedRewardPlanck ?? '0'))}`,
    );
  }
  console.log(`收款方数量: ${preview.recipients.length}`);
  for (const row of preview.recipients) {
    const reward = row.estimatedRewardPlanck == null ? '—' : formatNex(BigInt(row.estimatedRewardPlanck));
    console.log(
      `  [${row.role === 'validator' ? 'validator' : 'nominator'}] ${row.stash} | stake=${formatNex(BigInt(row.stakePlanck))} | estimated=${reward}`,
    );
  }
}

async function scanRewardedEvents(
  api: ApiPromise,
  lookbackBlocks: number,
  options?: ScanRewardedOptions,
): Promise<RewardedSummary> {
  if (lookbackBlocks === 0) {
    return {
      lookbackBlocks: 0,
      range: { from: 0, to: 0 },
      scannedRange: null,
      blocksScanned: 0,
      stoppedEarlyPruned: false,
      eventCount: 0,
      eventCountByStash: {},
      amountByStashPlanck: {},
      recentSamplesByStash: {},
      samples: [],
    };
  }

  const finalized = await api.rpc.chain.getFinalizedHead();
  const headHeader = await api.rpc.chain.getHeader(finalized);
  const end = (headHeader.number as unknown as { toNumber: () => number }).toNumber();
  const start = Math.max(1, end - lookbackBlocks + 1);
  const showProgress = options?.showProgress !== false;
  const progressEvery = Math.max(1, coerceNumber(process.env.EVENT_SCAN_PROGRESS_EVERY) ?? 100);

  const amountByStashPlanck: Record<string, bigint> = {};
  const eventCountByStash: Record<string, number> = {};
  const recentSamplesByStash: Record<string, Array<{ block: number; amountPlanck: string }>> = {};
  let eventCount = 0;
  const samples: Array<{ block: number; stash: string; amountPlanck: string }> = [];
  const maxSamples = 25;
  const maxRecentSamplesPerStash = 5;

  let blocksScanned = 0;
  let stoppedEarlyPruned = false;
  let minSuccessfulHeight = Number.POSITIVE_INFINITY;
  let maxSuccessfulHeight = Number.NEGATIVE_INFINITY;

  if (showProgress) {
    const span = end - start + 1;
    console.log(
      `〔事件扫描〕链尖高度 ${end}，自新向旧扫约 ${span} 个块（至 ${start}）；每 ${progressEvery} 块打印进度。RPC 慢时会久无输出属正常。`,
    );
  }

  // 从最新块向下扫：公共 RPC 常修剪历史状态，旧块会报 State discarded；自链尖向下可尽快汇总近期事件并在遇修剪时停止。
  for (let h = end; h >= start; h--) {
    try {
      const hash = await api.rpc.chain.getBlockHash(h);
      const eventsCodec = await api.query.system.events.at(hash);
      const records: EventRecord[] = (eventsCodec as any).toArray
        ? (eventsCodec as any).toArray()
        : Array.from(eventsCodec as unknown as Iterable<EventRecord>);

      blocksScanned++;
      minSuccessfulHeight = Math.min(minSuccessfulHeight, h);
      maxSuccessfulHeight = Math.max(maxSuccessfulHeight, h);

      if (showProgress && blocksScanned % progressEvery === 0) {
        console.log(`〔事件扫描〕已读 ${blocksScanned} 块，当前高度 ${h}，累计奖励事件 ${eventCount} 条`);
      }

      for (const record of records) {
        const ev = record.event;
        const section = ev.section.toString();
        const method = ev.method.toString();
        if (section !== 'staking' || method !== 'Rewarded') {
          continue;
        }
        const parsed = parseStakingRewardedEventData(ev.data);
        if (!parsed?.stash) {
          continue;
        }
        eventCount++;
        const { stash, amountPlanck: amount } = parsed;

        eventCountByStash[stash] = (eventCountByStash[stash] ?? 0) + 1;
        amountByStashPlanck[stash] = (amountByStashPlanck[stash] ?? 0n) + amount;

        const perStashSamples = (recentSamplesByStash[stash] ??= []);
        if (perStashSamples.length < maxRecentSamplesPerStash) {
          perStashSamples.push({ block: h, amountPlanck: amount.toString() });
        }

        if (samples.length < maxSamples) {
          samples.push({ block: h, stash, amountPlanck: amount.toString() });
        }
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (isStatePrunedRpcError(msg)) {
        stoppedEarlyPruned = true;
        if (showProgress) {
          console.log(
            `〔事件扫描〕高度 ${h} 无法读 events（状态已修剪或未知块），停止向更旧扫描；已成功 ${blocksScanned} 块。`,
          );
        }
        break;
      }
      // 其它 RPC 抖动：跳过该块继续
      continue;
    }
  }

  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(amountByStashPlanck)) {
    out[k] = v.toString();
  }

  const recentSamplesOut: Record<string, Array<{ block: number; amountPlanck: string }>> = {};
  for (const [stash, stashSamples] of Object.entries(recentSamplesByStash)) {
    recentSamplesOut[stash] = stashSamples;
  }

  const scannedRange =
    blocksScanned > 0 && Number.isFinite(minSuccessfulHeight) && Number.isFinite(maxSuccessfulHeight)
      ? { from: minSuccessfulHeight, to: maxSuccessfulHeight }
      : null;

  if (showProgress) {
    console.log(`〔事件扫描〕完成：共读 ${blocksScanned} 块，奖励事件 ${eventCount} 条。`);
  }

  return {
    lookbackBlocks,
    range: { from: start, to: end },
    scannedRange,
    blocksScanned,
    stoppedEarlyPruned,
    eventCount,
    eventCountByStash,
    amountByStashPlanck: out,
    recentSamplesByStash: recentSamplesOut,
    samples,
  };
}

/* -------------------------------------------------------------------------- */
/*  Main                                                                       */
/* -------------------------------------------------------------------------- */

async function main(): Promise<void> {
  const { stashFilter, json, eventBlocks, previewEra, previewPage } = parseCli(process.argv.slice(2));
  const api = await connectApi(process.env.WS_URL);

  try {
    if (!json) {
      console.log('〔质押审计〕已连接，正在查询链信息与验证人质押…');
    }
    const chain = await captureChainSnapshot(api);
    const activeEraCodec = await api.query.staking.activeEra();
    const activeEra = codecToJson(activeEraCodec) as { index: number };
    const activeEraIndex = coerceNumber(activeEra.index) ?? 0;
    const historyDepth = api.consts.staking.historyDepth.toNumber();

    const eraStart = Math.max(0, activeEraIndex - historyDepth);
    const eraEndInclusive = activeEraIndex - 1;
    const payoutEras = buildPayoutEraRange(activeEraIndex, historyDepth);

    const sessionVals = await api.query.session.validators();
    let validators = sessionVals.toArray().map((a) => a.toString());
    if (stashFilter) {
      validators = validators.filter((a: string) => a === stashFilter);
      if (validators.length === 0) {
        validators = [stashFilter];
      }
    }

    const phaseA: Array<{
      stash: string;
      controller: string | null;
      ledger: Record<string, unknown> | null;
      sessionIndex: number;
      exposureActiveEra: unknown;
      validatorPrefs: unknown;
    }> = [];

    for (let i = 0; i < validators.length; i++) {
      const stash = validators[i];
      const bonded = await api.query.staking.bonded(stash);
      const controller = bonded.isEmpty ? null : bonded.toString();
      let ledgerJson: Record<string, unknown> | null = null;
      if (controller) {
        const ledger = await api.query.staking.ledger(controller);
        if (!ledger.isEmpty) {
          ledgerJson = codecToJson(ledger) as Record<string, unknown>;
        }
      }

      const exposureActiveEra = codecToJson(await (api.query.staking as any).erasStakersOverview(activeEraIndex, stash));
      const validatorPrefs = codecToJson(await api.query.staking.validators(stash));

      phaseA.push({
        stash,
        controller,
        ledger: ledgerJson,
        sessionIndex: i,
        exposureActiveEra,
        validatorPrefs,
      });
    }

    const phaseB: Record<string, UnclaimedEraRow[]> = {};
    for (const row of phaseA) {
      phaseB[row.stash] = await listUnclaimedEraPages(api, row.stash, row.ledger, payoutEras);
    }

    const preview =
      previewEra != null && previewPage != null
        ? await getStakingEraPagePreview(
            api,
            stashFilter!,
            phaseA.find((row) => row.stash === stashFilter)?.ledger ?? null,
            previewEra,
            previewPage,
            payoutEras,
          )
        : null;

    const events = await scanRewardedEvents(api, eventBlocks, { showProgress: !json });

    const report = {
      chain,
      staking: {
        activeEra: activeEraIndex,
        historyDepth,
        payoutEraRangeInclusive:
          eraEndInclusive >= eraStart ? { start: eraStart, end: eraEndInclusive } : { start: null, end: null },
        note:
          '未领取判定：优先 ErasStakersOverview + ClaimedRewards（分页），否则 ErasStakers + legacyClaimedRewards（旧版）。可能与极少数 legacy clipped 边界情况不完全一致。',
      },
      phaseA,
      phaseB,
      preview,
      events,
    };

    if (json) {
      console.log(JSON.stringify(report, null, 2));
      return;
    }

    console.log(`链: ${chain.chain} | ${chain.specName} 规格版本 ${chain.specVersion}`);
    console.log(`当前 era: ${activeEraIndex} | historyDepth（历史深度）: ${historyDepth}`);
    console.log(`已扫描可 payout 的 era 数: ${payoutEras.length}（范围 ${eraStart}..${eraEndInclusive}）`);
    console.log(`验证人数量: ${validators.length}${stashFilter ? `（筛选 stash ${stashFilter}）` : ''}`);

    for (const v of phaseA) {
      console.log('\n────────────────────────────────────────');
      console.log(`资金账户（stash）${v.stash}  |  会话集序号 #${v.sessionIndex}`);
      console.log(`  控制账户（controller）: ${v.controller ?? '—'}`);
      if (v.ledger) {
        const total = bigFromHexOrDec(readObjectField(v.ledger, 'total'));
        const active = bigFromHexOrDec(readObjectField(v.ledger, 'active'));
        console.log(`  质押账本总额: ${formatNex(total)} | 激活: ${formatNex(active)}`);
      } else {
        console.log('  质押账本: —');
      }
      console.log(`  当前活跃 era ${activeEraIndex} 曝光概览（overview）: ${JSON.stringify(v.exposureActiveEra)}`);
      console.log(`  验证人偏好: ${JSON.stringify(v.validatorPrefs)}`);

      const unclaimed = phaseB[v.stash] ?? [];
      if (unclaimed.length === 0) {
        console.log('  未领取 era: 窗口内无（或无曝光 / 已全部领取）。');
      } else {
        console.log(`  未领取 era（${unclaimed.length} 个）:`);
        for (const u of unclaimed) {
          console.log(
            `    era ${u.era} [${u.path === 'paged' ? '分页' : '旧版'}] 总页数 ${u.pageCount} | 未领页 ${JSON.stringify(u.missingPages)} | 已领页 ${JSON.stringify(u.claimedPages)} | 曝光总量 ${formatNex(BigInt(u.exposureTotalPlanck))}`,
          );
        }
      }

      const rewardEventCount = events.eventCountByStash[v.stash] ?? 0;
      const rewardAmountPlanck = BigInt(events.amountByStashPlanck[v.stash] ?? '0');
      const rewardSamples = events.recentSamplesByStash[v.stash] ?? [];
      if (eventBlocks === 0) {
        console.log('  最近奖励事件汇总: 已禁用扫描（--event-blocks 0）。');
      } else if (rewardEventCount === 0) {
        console.log('  最近奖励事件汇总: 在已扫描窗口内未命中该验证人奖励事件。');
      } else {
        console.log(
          `  最近奖励事件汇总: ${rewardEventCount} 条 | 合计 +${formatNex(rewardAmountPlanck)}${rewardSamples.length > 0 ? ` | 最近样本 ${rewardSamples.length} 条` : ''}`,
        );
        for (const sample of rewardSamples) {
          console.log(`    高度 ${sample.block}  +${formatNex(BigInt(sample.amountPlanck))}`);
        }
      }
    }

    if (preview) {
      renderPreviewText(preview);
    }

    console.log('\n════════════════════════════════════════');
    console.log(`质押奖励事件：请求回溯约 ${events.lookbackBlocks} 个块，命中 ${events.eventCount} 条`);
    if (events.lookbackBlocks > 0) {
      console.log(
        `请求高度区间: ${events.range.from} … ${events.range.to}（自链尖向旧；远端块可能因节点修剪读不到）`,
      );
      if (events.scannedRange) {
        console.log(
          `实际已读到链上事件的区块: ${events.blocksScanned} 个，高度约 ${events.scannedRange.from} … ${events.scannedRange.to}`,
        );
      } else {
        console.log('未能读取任何区块的事件记录（可能 RPC 失败或窗口内无可用状态）。');
      }
      if (events.stoppedEarlyPruned) {
        console.log(
          '说明: 遇到「状态已丢弃 / 未知块」等已停止向更旧高度继续扫。要看更久历史请换归档节点，或仅缩小窗口看近期。',
        );
      }
      const entries = Object.entries(events.amountByStashPlanck).sort((a, b) => {
        const da = BigInt(a[1]);
        const db = BigInt(b[1]);
        return da === db ? 0 : da < db ? 1 : -1;
      });
      const grandTotal = entries.reduce((sum, [, planck]) => sum + BigInt(planck), 0n);
      console.log(`本窗口内奖励金额合计（各资金账户之和）: ${formatNex(grandTotal)}`);
      console.log('按资金账户汇总（由多到少，至多显示 40 个）:');
      for (const [stash, amt] of entries.slice(0, 40)) {
        console.log(`  ${stash}  +${formatNex(BigInt(amt))}`);
      }
      if (entries.length > 40) {
        console.log(`  … 另有 ${entries.length - 40} 个账户未列出`);
      }
      console.log('事件样本（块高 · 收款地址 · 该条金额）:');
      for (const s of events.samples) {
        console.log(`  高度 ${s.block}  ${s.stash}  +${formatNex(BigInt(s.amountPlanck))}`);
      }
    }
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exitCode = 1;
});
