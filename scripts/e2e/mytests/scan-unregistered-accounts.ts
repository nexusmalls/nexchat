#!/usr/bin/env tsx
/**
 * 扫描三个账户文件，找出在指定 Entity 中尚未注册为会员的账户
 * Scan three account files to find accounts not yet registered as members in the given Entity.
 *
 * 用法 / Usage:
 *   node --import tsx mytests/scan-unregistered-accounts.ts [entityId]
 *
 * 环境变量 / Env vars:
 *   WS_URL     — WebSocket 端点（默认: ws://127.0.0.1:9944）
 *   ENTITY_ID  — 实体 ID（默认: 100000）
 */

process.env.WS_URL ??= 'ws://127.0.0.1:9944';

import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';

const ENTITY_ID = Number(process.argv[2] ?? process.env.ENTITY_ID ?? '100000');

const FRAMEWORK_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../framework');

const ACCOUNT_FILES = [
  path.join(FRAMEWORK_DIR, 'test-accounts-2026-03-20T00-37-56-148Z.json'),
  path.join(FRAMEWORK_DIR, 'test-accounts-2026-03-20T00-38-47-605Z.json'),
  path.join(FRAMEWORK_DIR, 'test-accounts-2026-03-20T01-03-22-751Z.json'),
];

interface AccountEntry {
  index: number;
  name: string;
  mnemonic: string;
  address: string;
}

interface AccountFile {
  accounts: AccountEntry[];
}

type MemberStatus = 'not_member' | 'member' | 'error';

interface AccountResult {
  file: string;
  index: number;
  address: string;
  status: MemberStatus;
  levelId?: number;
  directReferrals?: number;
  referrer?: string | null;
  errorMsg?: string;
}

async function main(): Promise<void> {
  console.log(`\n${'═'.repeat(76)}`);
  console.log(`  扫描未注册账户 | Scan Unregistered Accounts   Entity ID: ${ENTITY_ID}`);
  console.log(`${'═'.repeat(76)}\n`);

  const api = await connectApi();

  try {
    const results: AccountResult[] = [];

    for (const filePath of ACCOUNT_FILES) {
      const fileName = path.basename(filePath);
      const raw = await readFile(filePath, 'utf-8');
      const parsed = JSON.parse(raw) as AccountFile;
      console.log(`  扫描文件 / Scanning: ${fileName}  (${parsed.accounts.length} 账户)`);

      for (const entry of parsed.accounts) {
        let status: MemberStatus = 'not_member';
        let levelId: number | undefined;
        let directReferrals: number | undefined;
        let referrer: string | null | undefined;
        let errorMsg: string | undefined;

        try {
          const memberRaw = await (api.query as any).entityMember.entityMembers(ENTITY_ID, entry.address);
          if ((memberRaw as any).isSome) {
            const member = codecToJson<Record<string, unknown>>((memberRaw as any).unwrap());
            status = 'member';
            levelId = coerceNumber(readObjectField(member, 'customLevelId', 'custom_level_id')) ?? 0;
            directReferrals = coerceNumber(readObjectField(member, 'directReferrals', 'direct_referrals')) ?? 0;
            const ref = readObjectField(member, 'referrer');
            if (ref == null || ref === '') {
              referrer = null;
            } else if (typeof ref === 'object' && ref !== null) {
              // Option<AccountId> encoded as { some: addr } or null
              const inner = (ref as any).some ?? (ref as any).Some ?? (ref as any).value ?? ref;
              referrer = typeof inner === 'string' ? inner : JSON.stringify(inner);
            } else {
              referrer = String(ref);
            }
          }
        } catch (e) {
          status = 'error';
          errorMsg = String(e);
        }

        results.push({
          file: fileName,
          index: entry.index,
          address: entry.address,
          status,
          levelId,
          directReferrals,
          referrer,
          errorMsg,
        });
      }
    }

    // ── 汇总输出 ──
    const notMembers  = results.filter(r => r.status === 'not_member');
    const members     = results.filter(r => r.status === 'member');
    const errors      = results.filter(r => r.status === 'error');

    console.log(`\n${'─'.repeat(76)}`);
    console.log(`  扫描结果汇总 / Summary`);
    console.log(`${'─'.repeat(76)}`);
    console.log(`  总账户数:       ${results.length}`);
    console.log(`  未注册 (可用):  ${notMembers.length}`);
    console.log(`  已注册会员:     ${members.length}`);
    console.log(`  查询出错:       ${errors.length}`);

    // ── 已注册会员明细 ──
    if (members.length > 0) {
      console.log(`\n${'─'.repeat(76)}`);
      console.log(`  已注册会员列表 / Registered Members`);
      console.log(`${'─'.repeat(76)}`);
      console.log(`  ${'文件'.padEnd(50)}  ${'idx'.padStart(3)}  ${'Level'.padStart(5)}  ${'Direct'.padStart(6)}  推荐人 / Referrer`);
      console.log(`  ${'─'.repeat(74)}`);
      for (const r of members) {
        const ref = r.referrer
          ? `${r.referrer.slice(0, 12)}...${r.referrer.slice(-6)}`
          : '(无)';
        console.log(
          `  ${r.file.padEnd(50)}  ${String(r.index).padStart(3)}  ` +
          `${String(r.levelId).padStart(5)}  ${String(r.directReferrals).padStart(6)}  ${ref}`
        );
      }
    }

    // ── 未注册账户明细 ──
    console.log(`\n${'─'.repeat(76)}`);
    console.log(`  未注册账户列表（可用作下级）/ Unregistered Accounts (available as sub-accounts)`);
    console.log(`${'─'.repeat(76)}`);
    console.log(`  ${'文件'.padEnd(50)}  ${'idx'.padStart(3)}  地址 / Address`);
    console.log(`  ${'─'.repeat(74)}`);
    for (const r of notMembers) {
      console.log(`  ${r.file.padEnd(50)}  ${String(r.index).padStart(3)}  ${r.address}`);
    }

    // ── 为 upgrade-via-referral-chain.ts 生成建议配置 ──
    console.log(`\n${'─'.repeat(76)}`);
    console.log(`  建议配置 / Suggested Configuration for upgrade-via-referral-chain.ts`);
    console.log(`${'─'.repeat(76)}`);

    const NUM_L1     = 5;
    const L2_PER_L1  = 2;
    const needed     = NUM_L1 + NUM_L1 * L2_PER_L1;  // 15

    // 把未注册账户按文件分组，优先同文件连续选取，减少配置复杂度
    // 简化：直接在合并池 notMembers 中按顺序找 index
    // 合并池顺序与脚本一致：FILE_A[0..19] | FILE_B[0..19] | FILE_C[0..19]
    // poolIndex = fileOffset * 20 + entry.index（这里 fileOffset 按扫描顺序）
    const fileOffsets: Record<string, number> = {
      'test-accounts-2026-03-20T00-37-56-148Z.json': 0,
      'test-accounts-2026-03-20T00-38-47-605Z.json': 20,
      'test-accounts-2026-03-20T01-03-22-751Z.json': 40,
    };

    const notMemberPoolIdxs = notMembers.map(r => ({
      poolIndex: fileOffsets[r.file] + r.index,
      address: r.address,
      file: r.file,
      index: r.index,
    })).sort((a, b) => a.poolIndex - b.poolIndex);

    if (notMemberPoolIdxs.length < needed) {
      console.log(`  ⚠  未注册账户数量不足（需要 ${needed} 个，可用 ${notMemberPoolIdxs.length} 个）`);
      console.log(`  ⚠  Insufficient unregistered accounts (need ${needed}, available ${notMemberPoolIdxs.length})`);
    } else {
      const firstAvail = notMemberPoolIdxs[0].poolIndex;
      console.log(`  需要 ${needed} 个未注册账户（${NUM_L1} 个 L1 + ${NUM_L1 * L2_PER_L1} 个 L2）`);
      console.log(`  Need ${needed} unregistered accounts (${NUM_L1} L1s + ${NUM_L1 * L2_PER_L1} L2s)\n`);
      console.log(`  推荐设置 / Recommended env vars:`);
      console.log(`    SUB_POOL_START=${firstAvail}   # 合并池中第一个未注册账户的 index`);
      console.log(``);
      console.log(`  将使用以下账户 / Will use these accounts:`);
      const selected = notMemberPoolIdxs.slice(0, needed);
      for (let i = 0; i < NUM_L1; i++) {
        const s = selected[i];
        console.log(`    L1[${i + 1}]  pool:${s.poolIndex}  (${s.file} index ${s.index})  ${s.address}`);
      }
      for (let i = 0; i < NUM_L1 * L2_PER_L1; i++) {
        const s = selected[NUM_L1 + i];
        const l1Owner = Math.floor(i / L2_PER_L1) + 1;
        console.log(`    L2[${i + 1}] (→L1[${l1Owner}])  pool:${s.poolIndex}  (${s.file} index ${s.index})  ${s.address}`);
      }

      // 检查连续性（判断 pool index 是否连续，如果不连续给出警告）
      const poolIdxList = selected.map(s => s.poolIndex);
      const minIdx = poolIdxList[0];
      const maxIdx = poolIdxList[poolIdxList.length - 1];
      const isContiguous = maxIdx - minIdx === needed - 1 &&
        poolIdxList.every((v, i) => i === 0 || v === poolIdxList[i - 1] + 1);

      if (!isContiguous) {
        console.log(`\n  ⚠  所选账户 pool index 不连续！脚本依赖连续 index 构建 L1/L2 映射。`);
        console.log(`  ⚠  Selected pool indices are NOT contiguous! Script expects sequential indices.`);
        console.log(`  ⚠  脚本将按顺序映射：L1[0..${NUM_L1 - 1}] → pool[${minIdx}..${minIdx + NUM_L1 - 1}]，`);
        console.log(`  ⚠                      L2[0..${NUM_L1 * L2_PER_L1 - 1}] → pool[${minIdx + NUM_L1}..${maxIdx}]。`);
        console.log(`  ⚠  但实际可用的连续起点是 pool:${minIdx}，若中间有已注册账户则会出错。`);
        console.log(`  ⚠  请检查下方输出，确认 pool[${minIdx}..${maxIdx}] 中所有账户均未注册。`);
        console.log(``);
        // 输出 minIdx..maxIdx 范围内所有账户的状态
        const rangeResults = results.filter(r => {
          const pi = fileOffsets[r.file] + r.index;
          return pi >= minIdx && pi <= maxIdx;
        }).sort((a, b) => (fileOffsets[a.file] + a.index) - (fileOffsets[b.file] + b.index));

        console.log(`  pool[${minIdx}..${maxIdx}] 范围内账户状态:`);
        for (const r of rangeResults) {
          const pi = fileOffsets[r.file] + r.index;
          const flag = r.status === 'not_member' ? '✓ 未注册' : '✗ 已注册';
          console.log(`    pool:${String(pi).padStart(2)}  ${flag}  ${r.address}`);
        }
      } else {
        console.log(`\n  ✓ pool index 连续（${minIdx}..${maxIdx}），可直接用 SUB_POOL_START=${minIdx}`);
      }
    }

    if (errors.length > 0) {
      console.log(`\n${'─'.repeat(76)}`);
      console.log(`  查询出错的账户 / Query Errors`);
      for (const r of errors) {
        console.log(`  ${r.file} index ${r.index}: ${r.errorMsg}`);
      }
    }

    console.log(`\n${'═'.repeat(76)}\n`);

  } finally {
    await disconnectApi(api);
  }
}

main().catch((e: unknown) => {
  console.error('\n[ERROR]', e instanceof Error ? e.message : String(e));
  process.exit(1);
});
