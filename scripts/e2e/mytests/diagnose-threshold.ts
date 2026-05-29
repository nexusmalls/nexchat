#!/usr/bin/env tsx
/**
 * 诊断购物余额阈值问题
 * 查询: RepurchaseConfig, 购物余额, NEX价格, 并计算折算值
 */

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { connectApi, disconnectApi } from '../framework/api.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

const MNEMONIC = process.argv.find((_, i, a) => a[i - 1] === '-m' || a[i - 1] === '--mnemonic') ?? '';
const ENTITY_ID = Number(process.argv.find((_, i, a) => a[i - 1] === '-e' || a[i - 1] === '--entity') ?? '100000');

async function main() {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const signer = keyring.addFromMnemonic(MNEMONIC);
  console.log(`会员地址: ${signer.address}`);
  console.log(`实体 ID: ${ENTITY_ID}\n`);

  const api = await connectApi('ws://127.0.0.1:9944');

  try {
    const cc = (api.query as any).commissionCore;
    const loyalty = (api.query as any).entityLoyalty;
    const nexMarket = (api.query as any).nexMarket;

    // 1. RepurchaseConfig
    console.log('=== RepurchaseConfig ===');
    const rcRaw = await cc.repurchaseConfigs(ENTITY_ID);
    if (rcRaw.isNone) {
      console.log('  RepurchaseConfig: 未设置 (None)');
    } else {
      const rc = rcRaw.unwrap();
      console.log(`  min_package_usdt:          ${rc.minPackageUsdt?.toString() ?? rc.min_package_usdt?.toString() ?? '?'}`);
      console.log(`  enforced:                  ${rc.enforced?.toString() ?? '?'}`);
      console.log(`  auto_order:                ${rc.autoOrder?.toString() ?? rc.auto_order?.toString() ?? '?'}`);
      console.log(`  default_product_id:        ${rc.defaultProductId?.toString() ?? rc.default_product_id?.toString() ?? '?'}`);
      console.log(`  shopping_balance_ttl:      ${rc.shoppingBalanceTtlBlocks?.toString() ?? rc.shopping_balance_ttl_blocks?.toString() ?? '?'}`);
      console.log(`  max_shopping_balance_usdt: ${rc.maxShoppingBalanceUsdt?.toString() ?? rc.max_shopping_balance_usdt?.toString() ?? '?'}`);

      const maxUsdt = BigInt(rc.maxShoppingBalanceUsdt?.toString() ?? rc.max_shopping_balance_usdt?.toString() ?? '0');
      console.log(`  → 阈值解读: ${Number(maxUsdt) / 1e6} USDT (精度 10^6)`);
    }

    // 2. 购物余额 (NEX)
    console.log('\n=== 购物余额 (NEX) ===');
    let shoppingBal = 0n;
    if (loyalty?.memberShoppingBalance) {
      const balRaw = await loyalty.memberShoppingBalance(ENTITY_ID, signer.address);
      shoppingBal = BigInt(balRaw.toString());
      console.log(`  MemberShoppingBalance: ${shoppingBal.toString()}`);
      console.log(`  → 折算 NEX: ${Number(shoppingBal) / 1e12} NEX`);
    } else {
      console.log('  entityLoyalty.memberShoppingBalance 不可用');
      // fallback: check commissionCore storage
      if (cc.memberShoppingBalance) {
        const balRaw = await cc.memberShoppingBalance(ENTITY_ID, signer.address);
        shoppingBal = BigInt(balRaw.toString());
        console.log(`  commissionCore.memberShoppingBalance: ${shoppingBal.toString()}`);
        console.log(`  → 折算 NEX: ${Number(shoppingBal) / 1e12} NEX`);
      }
    }

    // 3. NEX 价格
    console.log('\n=== NEX 价格 ===');

    // LastTradePrice
    let lastTradePrice = 0n;
    if (nexMarket?.lastTradePrice) {
      const raw = await nexMarket.lastTradePrice();
      lastTradePrice = BigInt(raw.toString());
      console.log(`  LastTradePrice: ${lastTradePrice.toString()}`);
      console.log(`  → 解读: ${Number(lastTradePrice) / 1e6} USDT/NEX (精度 10^6)`);
    }

    // PriceProtectionStore (initial_price fallback)
    if (nexMarket?.priceProtectionStore) {
      const ppRaw = await nexMarket.priceProtectionStore();
      console.log(`  PriceProtectionStore: ${JSON.stringify(ppRaw.toJSON())}`);
    }

    // 4. 计算折算
    console.log('\n=== 折算计算 ===');
    const nexUsdtRate = lastTradePrice > 0n ? lastTradePrice : 0n;
    console.log(`  使用价格: ${nexUsdtRate.toString()} (${Number(nexUsdtRate) / 1e6} USDT/NEX)`);
    console.log(`  购物余额: ${shoppingBal.toString()} (${Number(shoppingBal) / 1e12} NEX)`);

    // 链上公式: bal_usdt = shopping_balance * nex_usdt_rate / 10^12
    const balUsdt = (shoppingBal * nexUsdtRate) / 1_000_000_000_000n;
    console.log(`  bal_usdt = shopping_bal * rate / 10^12`);
    console.log(`          = ${shoppingBal} * ${nexUsdtRate} / 10^12`);
    console.log(`          = ${balUsdt.toString()}`);
    console.log(`  → 折算 USDT: ${Number(balUsdt) / 1e6} USDT`);

    // 5. 阈值比较
    const rcRaw2 = await cc.repurchaseConfigs(ENTITY_ID);
    if (!rcRaw2.isNone) {
      const rc = rcRaw2.unwrap();
      const maxUsdt = BigInt(rc.maxShoppingBalanceUsdt?.toString() ?? rc.max_shopping_balance_usdt?.toString() ?? '0');
      console.log(`\n=== 阈值比较 ===`);
      console.log(`  bal_usdt:                  ${balUsdt.toString()} (${Number(balUsdt) / 1e6} USDT)`);
      console.log(`  max_shopping_balance_usdt: ${maxUsdt.toString()} (${Number(maxUsdt) / 1e6} USDT)`);
      console.log(`  bal_usdt <= max?           ${balUsdt <= maxUsdt}`);
      if (balUsdt > maxUsdt) {
        console.log(`  *** 超出 ${Number(balUsdt - maxUsdt) / 1e6} USDT → ShoppingBalanceExceedsThreshold ***`);
      }
    }

    // 6. TWAP 检查
    console.log('\n=== TWAP 信息 ===');
    if ((api.call as any).nexMarketApi?.calculateTwap) {
      try {
        const twap1h = await (api.call as any).nexMarketApi.calculateTwap('OneHour');
        console.log(`  TWAP (1h): ${twap1h.toString()}`);
      } catch {
        console.log('  TWAP 查询不可用');
      }
    } else {
      console.log('  nexMarketApi runtime API 不可用');
    }

  } finally {
    await disconnectApi(api);
  }
}

main().catch(console.error);
