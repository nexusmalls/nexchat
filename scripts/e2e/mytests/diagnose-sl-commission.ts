#!/usr/bin/env tsx
/**
 * SingleLine 佣金 0 收益深度诊断脚本
 *
 * 诊断为什么账户在有下线订单的情况下没有收到 SingleLine 佣金
 */

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, codecToHuman, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex, asBigInt } from '../framework/units.js';

const ACCOUNT = 'X4WMbyCMgCpMJzwg1cdWQuPRRfQiu8ifrJmfLdurviJcTXW94';
const ENTITY_ID = 100000;

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

function ln(char = '─', len = 80): string { return char.repeat(len); }
function header(text: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${text}`);
  console.log(ln('═'));
}
function sub(text: string): void {
  console.log(`\n  ${ln('─', 68)}`);
  console.log(`  ${text}`);
  console.log(`  ${ln('─', 68)}`);
}

async function main(): Promise<void> {
  const api = await connectApi();

  try {
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();
    const cc = (api.query as any).commissionCore;
    const sl = (api.query as any).commissionSingleLine;
    const ml = (api.query as any).commissionMultiLevel;
    const tx = (api.query as any).entityTransaction;
    const mb = (api.query as any).entityMember;

    header(`SingleLine 佣金 0 收益深度诊断 — 区块 #${currentBlock}`);
    console.log(`  目标账户: ${ACCOUNT}`);
    console.log(`  实体 ID:  ${ENTITY_ID}`);

    // ═══════════════════════════════════════════════════════════════
    // 1. 查看所有订单 — 找出哪些订单存在
    // ═══════════════════════════════════════════════════════════════
    sub('1. 查询所有订单 (nextOrderId 向下扫描)');

    const nextOrderIdRaw = await tx.nextOrderId();
    const nextOrderId = coerceNumber(codecToJson(nextOrderIdRaw)) ?? 0;
    console.log(`  nextOrderId = ${nextOrderId}`);

    const allOrders: any[] = [];
    for (let oid = 0; oid < nextOrderId; oid++) {
      const orderRaw = await tx.orders(oid);
      if (!orderRaw || (orderRaw as any).isNone) continue;
      const order = codecToJson<Record<string, unknown>>((orderRaw as any).unwrap());
      const entityId = coerceNumber(readObjectField(order, 'entityId', 'entity_id'))!;
      if (entityId !== ENTITY_ID) continue;

      const buyer  = String(readObjectField(order, 'buyer') ?? '');
      const seller = String(readObjectField(order, 'seller') ?? '');
      const status = String(readObjectField(order, 'status') ?? 'Unknown');
      const totalAmount = asBigInt(readObjectField(order, 'totalAmount', 'total_amount') ?? 0);
      const platformFee = asBigInt(readObjectField(order, 'platformFee', 'platform_fee') ?? 0);
      const createdAt = coerceNumber(readObjectField(order, 'createdAt', 'created_at')) ?? 0;
      const completedAt = readObjectField(order, 'completedAt', 'completed_at');
      const payer  = readObjectField(order, 'payer') as string | null;
      const paymentAsset = String(readObjectField(order, 'paymentAsset', 'payment_asset') ?? 'Native');

      allOrders.push({ oid, buyer, seller, payer, status, totalAmount, platformFee, createdAt, completedAt, paymentAsset, raw: order });

      const isRelated = buyer === ACCOUNT || seller === ACCOUNT || payer === ACCOUNT;
      const tag = isRelated ? ' <<<' : '';
      console.log(`  订单 #${oid}: 买家=${shortAddr(buyer)} 卖家=${shortAddr(seller)} 状态=${status} 金额=${formatNex(totalAmount)} 平台费=${formatNex(platformFee)} paymentAsset=${paymentAsset} 创建=#${createdAt} 完成=${completedAt != null ? `#${coerceNumber(completedAt)}` : '(未完成)'}${tag}`);
    }

    // ═══════════════════════════════════════════════════════════════
    // 2. 佣金配置 — CommissionConfigs
    // ═══════════════════════════════════════════════════════════════
    sub('2. CommissionCore 配置');

    const commConfig = codecToJson<Record<string, unknown>>(
      await cc.commissionConfigs(ENTITY_ID),
    );
    console.log(`  CommissionConfigs: ${JSON.stringify(commConfig, null, 2)}`);

    const maxRate = coerceNumber(readObjectField(commConfig, 'maxCommissionRate', 'max_commission_rate')) ?? 0;
    const enabled = readObjectField(commConfig, 'enabled');
    console.log(`  maxCommissionRate = ${maxRate} bps (${(maxRate/100).toFixed(1)}%)`);
    console.log(`  enabled plugins = ${JSON.stringify(enabled)}`);

    // 全局暂停
    const globalPaused = codecToJson<boolean>(await cc.globalCommissionPaused()) ?? false;
    const wdPaused = codecToJson<boolean>(await cc.withdrawalPaused(ENTITY_ID)) ?? false;
    console.log(`  globalCommissionPaused = ${globalPaused}`);
    console.log(`  withdrawalPaused(${ENTITY_ID}) = ${wdPaused}`);

    // ═══════════════════════════════════════════════════════════════
    // 3. SingleLine 配置 & 队列状态
    // ═══════════════════════════════════════════════════════════════
    sub('3. SingleLine 配置 & 队列');

    // Config
    let slConfig: any = null;
    try {
      const raw = await sl.singleLineConfigs(ENTITY_ID);
      if (raw && !(raw as any).isNone) {
        slConfig = codecToJson((raw as any).unwrap ? (raw as any).unwrap() : raw);
      }
    } catch(e) { console.log(`  SingleLineConfigs 查询失败: ${e}`); }
    console.log(`  SingleLineConfigs: ${JSON.stringify(slConfig, null, 2)}`);

    // Enabled
    let slEnabled = true;
    try {
      slEnabled = codecToJson<boolean>(await sl.singleLineEnabled(ENTITY_ID)) ?? true;
    } catch {}
    console.log(`  SingleLineEnabled = ${slEnabled}`);

    // Segment count
    let segCount = 0;
    try {
      segCount = coerceNumber(codecToJson(await sl.singleLineSegmentCount(ENTITY_ID))) ?? 0;
    } catch {}
    console.log(`  SingleLineSegmentCount = ${segCount}`);

    // 遍历所有 segments 列出成员
    console.log(`\n  SingleLine 队列成员:`);
    const allSlMembers: { segId: number; position: number; addr: string }[] = [];
    for (let seg = 0; seg < segCount; seg++) {
      const segMembers = codecToJson<string[]>(await sl.singleLineSegments(ENTITY_ID, seg)) ?? [];
      console.log(`    Segment ${seg} (${segMembers.length} 人): ${segMembers.map(shortAddr).join(', ')}`);
      segMembers.forEach((addr, pos) => {
        allSlMembers.push({ segId: seg, position: seg * 1000 + pos, addr: String(addr) });
      });
    }

    // 目标账户的 index
    const targetIndex = codecToJson(await sl.singleLineIndex(ENTITY_ID, ACCOUNT));
    console.log(`\n  目标账户 SingleLineIndex = ${JSON.stringify(targetIndex)}`);

    // RemovedMembers 检查
    let isRemoved = false;
    try {
      isRemoved = codecToJson<boolean>(await sl.removedMembers(ENTITY_ID, ACCOUNT)) ?? false;
    } catch {}
    console.log(`  目标账户 RemovedMembers = ${isRemoved}`);

    // 所有成员的 index
    console.log(`\n  所有成员的 SingleLineIndex:`);
    const uniqueAddrs = [...new Set(allSlMembers.map(m => m.addr))];
    for (const addr of uniqueAddrs) {
      const idx = codecToJson(await sl.singleLineIndex(ENTITY_ID, addr));
      const removed = codecToJson<boolean>(await sl.removedMembers(ENTITY_ID, addr)) ?? false;
      console.log(`    ${shortAddr(addr)}  index=${JSON.stringify(idx)}  removed=${removed}`);
    }

    // ═══════════════════════════════════════════════════════════════
    // 4. 检查每笔订单的佣金记录
    // ═══════════════════════════════════════════════════════════════
    sub('4. 每笔订单的佣金记录 (NEX)');

    for (const o of allOrders) {
      const recs = codecToJson<any[]>(await cc.orderCommissionRecords(o.oid)) ?? [];
      console.log(`\n  订单 #${o.oid} (status=${o.status}, 买家=${shortAddr(o.buyer)}):`);
      if (recs.length === 0) {
        console.log(`    (无佣金记录)`);
      } else {
        for (const rec of recs) {
          const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? '');
          const beneficiary = String(readObjectField(rec, 'beneficiary') ?? '');
          const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
          const level = coerceNumber(readObjectField(rec, 'level')) ?? 0;
          const status = String(readObjectField(rec, 'status') ?? '');
          const isTarget = beneficiary === ACCOUNT ? ' <<< TARGET' : '';
          console.log(`    ${ctype.padEnd(22)} L${level} beneficiary=${shortAddr(beneficiary)} amount=${formatNex(amount)} status=${status}${isTarget}`);
        }
      }
    }

    // ═══════════════════════════════════════════════════════════════
    // 5. 未分配资金池
    // ═══════════════════════════════════════════════════════════════
    sub('5. 未分配资金池');

    for (const o of allOrders) {
      const unalloc = codecToJson<any>(await cc.orderUnallocated(o.oid));
      if (unalloc) {
        const amount = asBigInt(readObjectField(unalloc, '2') ?? readObjectField(unalloc, 'amount') ?? 0);
        if (amount > 0n) {
          console.log(`  订单 #${o.oid}: 未分配 ${formatNex(amount)}`);
        }
      }
    }

    const entityUnalloc = asBigInt(codecToJson(await cc.unallocatedPool(ENTITY_ID)));
    console.log(`  实体未分配总额: ${formatNex(entityUnalloc)}`);

    // ═══════════════════════════════════════════════════════════════
    // 6. MultiLevel 配置
    // ═══════════════════════════════════════════════════════════════
    sub('6. MultiLevel 配置');

    let mlConfig: any = null;
    try {
      const raw = await ml.multiLevelConfigs(ENTITY_ID);
      if (raw && !(raw as any).isNone) {
        mlConfig = codecToJson((raw as any).unwrap ? (raw as any).unwrap() : raw);
      }
    } catch(e) { console.log(`  查询失败: ${e}`); }
    console.log(`  MultiLevelConfigs: ${JSON.stringify(mlConfig, null, 2)}`);

    let mlPaused = false;
    try {
      mlPaused = codecToJson<boolean>(await ml.globalPaused(ENTITY_ID)) ?? false;
    } catch {}
    console.log(`  MultiLevel paused = ${mlPaused}`);

    // ═══════════════════════════════════════════════════════════════
    // 7. 成员关系链 — referral 关系
    // ═══════════════════════════════════════════════════════════════
    sub('7. 成员注册 & 推荐关系');

    for (const addr of uniqueAddrs) {
      let profile: any = null;
      try {
        const raw = await mb.memberProfiles(ENTITY_ID, addr);
        if (raw && !(raw as any).isNone) {
          profile = codecToJson((raw as any).unwrap ? (raw as any).unwrap() : raw);
        }
      } catch {}

      let referrer: any = null;
      try {
        const raw = await mb.memberReferrers(ENTITY_ID, addr);
        if (raw && !(raw as any).isNone) {
          referrer = codecToJson((raw as any).unwrap ? (raw as any).unwrap() : raw);
        }
      } catch {}

      console.log(`  ${shortAddr(addr)}:`);
      if (profile) {
        const joinedAt = coerceNumber(readObjectField(profile, 'joinedAt', 'joined_at')) ?? 0;
        const level = coerceNumber(readObjectField(profile, 'levelId', 'level_id')) ?? 0;
        console.log(`    加入区块=#${joinedAt} 等级=${level}`);
      } else {
        console.log(`    (无 profile)`);
      }
      console.log(`    推荐人: ${referrer ? shortAddr(String(referrer)) : '(无/根节点)'}`);
    }

    // ═══════════════════════════════════════════════════════════════
    // 8. 查看每个成员的佣金统计
    // ═══════════════════════════════════════════════════════════════
    sub('8. 所有成员的 CommissionCore 佣金统计');

    for (const addr of uniqueAddrs) {
      const stats = codecToJson<Record<string, unknown>>(
        await cc.memberCommissionStats(ENTITY_ID, addr),
      );
      const earned = asBigInt(readObjectField(stats, 'totalEarned', 'total_earned') ?? 0);
      const pending = asBigInt(readObjectField(stats, 'pending') ?? 0);
      const withdrawn = asBigInt(readObjectField(stats, 'withdrawn') ?? 0);
      const orderCount = coerceNumber(readObjectField(stats, 'orderCount', 'order_count')) ?? 0;
      const isTarget = addr === ACCOUNT ? ' <<< TARGET' : '';
      console.log(`  ${shortAddr(addr)}: earned=${formatNex(earned)} pending=${formatNex(pending)} withdrawn=${formatNex(withdrawn)} orders=${orderCount}${isTarget}`);
    }

    // ═══════════════════════════════════════════════════════════════
    // 9. SingleLine 逐成员统计
    // ═══════════════════════════════════════════════════════════════
    sub('9. 所有成员的 SingleLine 统计');

    for (const addr of uniqueAddrs) {
      let slStats: any = null;
      try {
        slStats = codecToJson(await sl.memberSingleLineStats(ENTITY_ID, addr));
      } catch {}
      const isTarget = addr === ACCOUNT ? ' <<< TARGET' : '';
      console.log(`  ${shortAddr(addr)}: ${JSON.stringify(slStats)}${isTarget}`);
    }

    // ═══════════════════════════════════════════════════════════════
    // 10. SingleLine Entity Stats
    // ═══════════════════════════════════════════════════════════════
    sub('10. SingleLine Entity Stats');
    let entitySlStats: any = null;
    try {
      entitySlStats = codecToJson(await sl.entitySingleLineStats(ENTITY_ID));
    } catch {}
    console.log(`  EntitySingleLineStats: ${JSON.stringify(entitySlStats)}`);

    // ═══════════════════════════════════════════════════════════════
    // 11. 深入检查 engine 逻辑 — CommissionCore enabled 字段
    // ═══════════════════════════════════════════════════════════════
    sub('11. CommissionCore.enabled 插件开关详情');

    // enabled 是一个结构体/enum 集合，看它的具体内容
    if (commConfig) {
      const enabledField = readObjectField(commConfig, 'enabled');
      console.log(`  enabled (raw): ${JSON.stringify(enabledField)}`);

      // 也用 human-readable 格式看看
      try {
        const humanConfig = codecToHuman(await cc.commissionConfigs(ENTITY_ID));
        console.log(`  commissionConfigs (human): ${JSON.stringify(humanConfig, null, 2)}`);
      } catch {}
    }

    // ═══════════════════════════════════════════════════════════════
    // 12. 检查 order 的 complete 事件 — 是不是 complete 后才触发佣金分配
    // ═══════════════════════════════════════════════════════════════
    sub('12. 订单完成状态分析');

    for (const o of allOrders) {
      const isComplete = o.status === 'Completed';
      const completedBlock = o.completedAt != null ? coerceNumber(o.completedAt) : null;
      console.log(`  订单 #${o.oid}: status=${o.status} completed=${completedBlock ?? '(null)'} 买家=${shortAddr(o.buyer)}${o.buyer === ACCOUNT ? ' (TARGET)' : ''}`);

      if (!isComplete) {
        console.log(`    [!!] 订单未完成 — 佣金尚未触发分配!`);
      }
    }

    // ═══════════════════════════════════════════════════════════════
    // SUMMARY
    // ═══════════════════════════════════════════════════════════════
    header('诊断总结');

    // 检查所有可能的原因
    const issues: string[] = [];

    if (globalPaused) issues.push('全局佣金已暂停 (globalCommissionPaused = true)');
    if (!slEnabled) issues.push('SingleLine 已暂停 (SingleLineEnabled = false)');
    if (isRemoved) issues.push('目标账户已从 SingleLine 队列中移除 (RemovedMembers = true)');
    if (maxRate === 0) issues.push('maxCommissionRate = 0, 佣金池为 0');

    if (slConfig) {
      const upRate = coerceNumber(readObjectField(slConfig, 'uplineRate', 'upline_rate')) ?? 0;
      const downRate = coerceNumber(readObjectField(slConfig, 'downlineRate', 'downline_rate')) ?? 0;
      if (upRate === 0 && downRate === 0) issues.push('SingleLine uplineRate 和 downlineRate 都是 0');
    } else {
      issues.push('SingleLine 未配置 (SingleLineConfigs 为空)');
    }

    // 检查订单完成状态
    const incompleteOrders = allOrders.filter(o => o.status !== 'Completed');
    if (incompleteOrders.length > 0) {
      issues.push(`${incompleteOrders.length} 笔订单未完成: [${incompleteOrders.map(o => `#${o.oid}(${o.status})`).join(', ')}] — 佣金仅在订单完成时分配`);
    }

    // 检查目标账户是否是买家（买家不会收到自己订单的佣金）
    const ordersByTarget = allOrders.filter(o => o.buyer === ACCOUNT);
    if (ordersByTarget.length > 0) {
      issues.push(`目标账户作为买家的订单: [${ordersByTarget.map(o => `#${o.oid}`).join(', ')}] — 买家不会收到自己订单的 SingleLine 佣金`);
    }

    // 检查 enabled 中是否包含 SingleLine
    const enabledField = readObjectField(commConfig, 'enabled');
    if (enabledField && typeof enabledField === 'object') {
      const enabledKeys = Object.keys(enabledField as object);
      const slRelated = enabledKeys.filter(k => k.toLowerCase().includes('single'));
      if (slRelated.length === 0) {
        // 检查是否是数组格式
        if (Array.isArray(enabledField)) {
          const hasSl = (enabledField as any[]).some(v =>
            typeof v === 'string' && v.toLowerCase().includes('single')
          );
          if (!hasSl) {
            issues.push(`enabled 插件列表中可能未包含 SingleLine: ${JSON.stringify(enabledField)}`);
          }
        }
      }
    }

    // 检查队列中是否只有目标账户
    if (allSlMembers.length <= 1) {
      issues.push(`SingleLine 队列中只有 ${allSlMembers.length} 人 — 至少需要买家的上线/下线在队列中才能分配`);
    }

    // 检查目标账户和买家的相对位置
    for (const o of allOrders) {
      if (o.buyer === ACCOUNT) continue; // 跳过自己的订单
      const buyerInQueue = allSlMembers.find(m => m.addr === o.buyer);
      const targetInQueue = allSlMembers.find(m => m.addr === ACCOUNT);
      if (buyerInQueue && targetInQueue) {
        const distance = Math.abs(buyerInQueue.position - targetInQueue.position);
        console.log(`  订单 #${o.oid}: 买家位置=${buyerInQueue.position} 目标位置=${targetInQueue.position} 距离=${distance}`);
      } else if (!buyerInQueue) {
        issues.push(`订单 #${o.oid} 的买家 ${shortAddr(o.buyer)} 不在 SingleLine 队列中`);
      }
    }

    if (issues.length === 0) {
      console.log('\n  未发现明显问题 — 需要进一步检查 engine 源码逻辑');
    } else {
      console.log(`\n  发现 ${issues.length} 个问题:\n`);
      issues.forEach((issue, idx) => {
        console.log(`  ${idx + 1}. ${issue}`);
      });
    }

    console.log(`\n${ln('═')}\n`);

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('Error:', err);
  process.exit(1);
});
