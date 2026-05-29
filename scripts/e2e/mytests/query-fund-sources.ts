#!/usr/bin/env tsx
/**
 * 实体资金来源审计脚本
 *
 * 用法:
 *   node --import tsx mytests/query-fund-sources.ts [entityId]
 *
 * 默认 entityId = 100000，可通过命令行参数或 ENTITY_ID 环境变量覆盖。
 */
process.env.WS_URL ??= 'ws://127.0.0.1:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson } from '../framework/codec.js';
import { readFreeBalance } from '../framework/accounts.js';
import { formatNex, asBigInt } from '../framework/units.js';
import { stringToU8a } from '@polkadot/util';
import { encodeAddress } from '@polkadot/util-crypto';

// ---------------------------------------------------------------------------
// 参数解析
// ---------------------------------------------------------------------------
const ENTITY_ID = Number(process.argv[2] ?? process.env.ENTITY_ID ?? 100000);
if (!Number.isInteger(ENTITY_ID) || ENTITY_ID <= 0) {
  console.error('用法: node --import tsx mytests/query-fund-sources.ts [entityId]');
  process.exit(1);
}

// ---------------------------------------------------------------------------
// Treasury 地址推导  PalletId("et/enty/") + entity_id (u64 LE) → 32-byte account
// ---------------------------------------------------------------------------
function deriveTreasuryAddress(entityId: number, ss58Format: number): string {
  const raw = new Uint8Array(32);
  raw.set(stringToU8a('modl'), 0);           // 4 bytes: "modl"
  raw.set(stringToU8a('et/enty/'), 4);       // 8 bytes: PalletId
  const dv = new DataView(raw.buffer);
  dv.setBigUint64(12, BigInt(entityId), true); // 8 bytes: entity_id LE
  // bytes 20-31 remain zero (padding)
  return encodeAddress(raw, ss58Format);
}

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------
const SEP = '='.repeat(72);
function section(n: number, title: string) {
  console.log(`\n${SEP}\n${n}. ${title}\n${SEP}`);
}

function kv(label: string, value: unknown, suffix = '') {
  const s = typeof value === 'bigint' ? formatNex(value) : JSON.stringify(value);
  console.log(`  ${label.padEnd(28)} ${s}${suffix ? '  ' + suffix : ''}`);
}

// ---------------------------------------------------------------------------
// 主流程 / Main
// ---------------------------------------------------------------------------
/**
 * 主入口：审计实体金库、佣金、订单与购物余额等资金来源与占用情况。
 */
async function main() {
  const api = await connectApi();
  const spec = `${api.runtimeVersion.specName} v${api.runtimeVersion.specVersion}`;
  console.log(`Chain: ${spec}  |  Entity: ${ENTITY_ID}`);

  try {
    // ===== 1. Entity 基本信息 =====
    section(1, '实体信息 / ENTITY INFO');
    const entityVal = await (api.query as any).entityRegistry.entities(ENTITY_ID);
    if (!(entityVal as any).isSome) {
      console.error(`  Entity ${ENTITY_ID} 不存在`);
      return;
    }
    const entity = codecToJson<any>((entityVal as any).unwrap());
    console.log(JSON.stringify(entity, null, 2));

    const sales = codecToJson<any>(await (api.query as any).entityRegistry.entitySales(ENTITY_ID));
    if (sales) console.log('  Sales:', JSON.stringify(sales));

    const payConfig = codecToJson<any>(await (api.query as any).entityRegistry.entityPaymentConfigs(ENTITY_ID));
    if (payConfig) console.log('  Payment:', JSON.stringify(payConfig));

    // ===== 2. Treasury 账户余额 =====
    section(2, '金库账户 / TREASURY ACCOUNT');
    const ss58 = api.registry.chainSS58 ?? 273;
    const treasuryAddr = deriveTreasuryAddress(ENTITY_ID, ss58);
    console.log(`  Address: ${treasuryAddr}`);

    const treasuryBalance = await readFreeBalance(api, treasuryAddr);
    const accountInfo = codecToJson<any>(await api.query.system.account(treasuryAddr));
    kv('Free', treasuryBalance);
    kv('Reserved', asBigInt(accountInfo?.data?.reserved ?? 0));
    kv('Frozen', asBigInt(accountInfo?.data?.frozen ?? 0));

    // ===== 3. Commission 存储 =====
    section(3, '佣金核心 / COMMISSION CORE');
    const cc = (api.query as any).commissionCore;

    const shopPending   = asBigInt(codecToJson(await cc.shopPendingTotal(ENTITY_ID)));
    const tokenPending  = asBigInt(codecToJson(await cc.tokenPendingTotal(ENTITY_ID)));
    const unallocPool   = asBigInt(codecToJson(await cc.unallocatedPool(ENTITY_ID)));
    const unallocToken  = asBigInt(codecToJson(await cc.unallocatedTokenPool(ENTITY_ID)));
    const pendingRefund = asBigInt(codecToJson(await cc.pendingRefundTotal(ENTITY_ID)));
    const pendingTkRef  = asBigInt(codecToJson(await cc.pendingTokenRefundTotal(ENTITY_ID)));

    kv('ShopPendingTotal (NEX)', shopPending);
    kv('TokenPendingTotal', tokenPending);
    kv('UnallocatedPool (NEX)', unallocPool);
    kv('UnallocatedTokenPool', unallocToken);
    kv('PendingRefundTotal', pendingRefund);
    kv('PendingTokenRefundTotal', pendingTkRef);

    const commConfig = codecToJson<any>(await cc.commissionConfigs(ENTITY_ID));
    if (commConfig) {
      console.log('\n  Commission Config:');
      console.log(JSON.stringify(commConfig, null, 2));
    }

    const shopTotals = codecToJson<any>(await cc.shopCommissionTotals(ENTITY_ID));
    if (shopTotals) kv('ShopCommissionTotals', shopTotals);

    // ===== 4. 成员佣金统计 =====
    section(4, '成员佣金统计 / MEMBER COMMISSION STATS');
    const memberStats = await cc.memberCommissionStats.entries(ENTITY_ID);
    console.log(`  ${memberStats.length} member(s) with NEX stats:`);
    for (const [key, value] of memberStats) {
      const account = key.args[1].toString();
      const data = codecToJson<any>(value);
      console.log(`\n  ${account}`);
      kv('totalEarned', asBigInt(data?.totalEarned ?? data?.total_earned ?? 0));
      kv('pending', asBigInt(data?.pending ?? 0));
      kv('withdrawn', asBigInt(data?.withdrawn ?? 0));
      kv('repurchased', asBigInt(data?.repurchased ?? 0));
      kv('orderCount', data?.orderCount ?? data?.order_count ?? 0);
    }

    const tokenStats = await cc.memberTokenCommissionStats.entries(ENTITY_ID);
    if (tokenStats.length > 0) {
      console.log(`\n  ${tokenStats.length} member(s) with Token stats:`);
      for (const [key, value] of tokenStats) {
        console.log(`  ${key.args[1].toString()}: ${JSON.stringify(codecToJson(value))}`);
      }
    }

    // ===== 5. Pool Reward =====
    section(5, '奖池分配 / POOL REWARD');
    const pr = (api.query as any).commissionPoolReward;

    const poolConfig = await pr.poolRewardConfigs(ENTITY_ID);
    if ((poolConfig as any).isSome) {
      console.log('  Config:', JSON.stringify(codecToJson((poolConfig as any).unwrap()), null, 2));
    } else {
      console.log('  Config: None');
    }

    const currentRound = await pr.currentRound(ENTITY_ID);
    if ((currentRound as any).isSome) {
      console.log('  CurrentRound:', JSON.stringify(codecToJson((currentRound as any).unwrap()), null, 2));
    }

    const lastRoundId = codecToJson(await pr.lastRoundId(ENTITY_ID));
    kv('LastRoundId', lastRoundId);

    const roundFunding = codecToJson<any>(await pr.currentRoundFunding(ENTITY_ID));
    if (roundFunding) {
      console.log('  CurrentRoundFunding:', JSON.stringify(roundFunding, null, 2));
    }

    const distStats = codecToJson<any>(await pr.distributionStatistics(ENTITY_ID));
    if (distStats) {
      console.log('  DistributionStats:', JSON.stringify(distStats, null, 2));
    }

    // ===== 6. 订单 =====
    section(6, '订单 / ORDERS');
    // 通过实体的 shop 找到关联订单
    const primaryShopId = entity.primaryShopId ?? entity.primary_shop_id;
    if (primaryShopId != null) {
      const shopOrders = codecToJson<number[]>(
        await (api.query as any).entityTransaction.shopOrders(primaryShopId)
      ) ?? [];
      console.log(`  Shop #${primaryShopId}: ${shopOrders.length} order(s) — ${JSON.stringify(shopOrders)}`);

      for (const oid of shopOrders.slice(-10)) { // 最近 10 笔
        const orderVal = await (api.query as any).entityTransaction.orders(oid);
        if ((orderVal as any).isSome) {
          const order = codecToJson<any>((orderVal as any).unwrap());
          const total = asBigInt(order?.totalAmount ?? order?.total_amount ?? 0);
          const status = order?.status ?? 'unknown';
          console.log(`  Order #${oid}: ${formatNex(total)}  status=${JSON.stringify(status)}`);
        }
      }
    }

    // ===== 7. Loyalty =====
    section(7, '忠诚度与购物金 / LOYALTY / SHOPPING BALANCE');
    const ly = (api.query as any).entityLoyalty;
    const shopShopping = asBigInt(codecToJson(await ly.shopShoppingTotal(ENTITY_ID)));
    const tokenShopping = asBigInt(codecToJson(await ly.tokenShoppingTotal(ENTITY_ID)));
    kv('ShopShoppingTotal (NEX)', shopShopping);
    kv('TokenShoppingTotal', tokenShopping);

    // ===== 8. 资金汇总 =====
    section(8, 'FUND SUMMARY');
    const committed = shopPending + unallocPool + pendingRefund;
    const unencumbered = treasuryBalance - committed;

    kv('Treasury Balance', treasuryBalance);
    console.log('  ---');
    kv('ShopPendingTotal', shopPending, '(待提现佣金)');
    kv('UnallocatedPool', unallocPool, '(未分配奖池)');
    kv('PendingRefundTotal', pendingRefund, '(待重试退款)');
    console.log('  ---');
    kv('Committed Total', committed);
    kv('Unencumbered Free', unencumbered, '(可自由使用)');

    if (tokenPending > 0n || unallocToken > 0n || pendingTkRef > 0n) {
      console.log('\n  Token:');
      kv('TokenPendingTotal', tokenPending);
      kv('UnallocatedTokenPool', unallocToken);
      kv('PendingTokenRefundTotal', pendingTkRef);
    }

  } finally {
    await disconnectApi(api);
  }
}

main().catch(e => { console.error(e); process.exit(1); });
