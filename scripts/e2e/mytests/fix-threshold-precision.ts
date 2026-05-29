#!/usr/bin/env tsx
/**
 * 修正购物余额阈值 & NEX 初始价格的精度
 *
 * 问题:
 *   - RepurchaseConfig.max_shopping_balance_usdt 存的是 12，实际精度应为 10^6，即 12 USDT = 12_000_000
 *   - RepurchaseConfig.min_package_usdt 存的是 11，同理应为 11_000_000
 *   - NEX initial_price 存的是 10/11，应为 10_000_000 / 11_000_000 (即 10~11 USDT)
 *
 * 修正步骤:
 *   1. Entity owner 调用 commissionCore.setRepurchaseConfig 修正 RepurchaseConfig (精度 ×10^6)
 *   2. Sudo 调用 nexMarket.setInitialPrice 修正 NEX 初始价格 (精度 ×10^6)
 *
 * 用法:
 *   npx tsx fix-threshold-precision.ts \
 *     --owner-mnemonic "entity owner 助记词" \
 *     --sudo-mnemonic "//Alice" \
 *     --entity 100000 \
 *     [--nex-price-usdt 11] \
 *     [--max-shopping-usdt 12] \
 *     [--min-package-usdt 11] \
 *     [--dry-run]
 */

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { connectApi, disconnectApi, submitTx } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

/* ------------------------------------------------------------------ */
/*  参数                                                                */
/* ------------------------------------------------------------------ */

interface Args {
  ownerMnemonic: string;
  sudoMnemonic: string;
  entityId: number;
  nexPriceUsdt: number;       // NEX 价格 (USDT 整数/小数)，如 11 表示 11 USDT
  maxShoppingUsdt: number;    // 购物余额阈值 (USDT)，如 12 表示 12 USDT
  minPackageUsdt: number;     // 最低复购套餐 (USDT)
  wsUrl: string;
  dryRun: boolean;
}

function parseArgs(): Args {
  const argv = process.argv.slice(2);
  let ownerMnemonic = '';
  let sudoMnemonic = '//Alice';
  let entityId = 100000;
  let nexPriceUsdt = 0;         // 0 = 从链上读取原值再乘 10^6
  let maxShoppingUsdt = 0;      // 0 = 从链上读取
  let minPackageUsdt = 0;       // 0 = 从链上读取
  let wsUrl = 'ws://127.0.0.1:9944';
  let dryRun = false;

  for (let i = 0; i < argv.length; i++) {
    switch (argv[i]) {
      case '--owner-mnemonic': case '-m':
        ownerMnemonic = argv[++i]; break;
      case '--sudo-mnemonic': case '-s':
        sudoMnemonic = argv[++i]; break;
      case '--entity': case '-e':
        entityId = Number(argv[++i]); break;
      case '--nex-price-usdt':
        nexPriceUsdt = Number(argv[++i]); break;
      case '--max-shopping-usdt':
        maxShoppingUsdt = Number(argv[++i]); break;
      case '--min-package-usdt':
        minPackageUsdt = Number(argv[++i]); break;
      case '--ws':
        wsUrl = argv[++i]; break;
      case '--dry-run':
        dryRun = true; break;
    }
  }

  if (!ownerMnemonic) {
    console.error('必须提供 --owner-mnemonic (-m)');
    process.exit(1);
  }

  return { ownerMnemonic, sudoMnemonic, entityId, nexPriceUsdt, maxShoppingUsdt, minPackageUsdt, wsUrl, dryRun };
}

/* ------------------------------------------------------------------ */
/*  工具                                                                */
/* ------------------------------------------------------------------ */

const USDT_PRECISION = 1_000_000;  // 精度 10^6

function ln(char = '─', len = 72): string { return char.repeat(len); }
function header(title: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${title}`);
  console.log(ln('═'));
}
function kv(label: string, value: string): void {
  console.log(`  ${label.padEnd(32)} ${value}`);
}

/* ------------------------------------------------------------------ */
/*  主流程                                                              */
/* ------------------------------------------------------------------ */

async function main(): Promise<void> {
  const args = parseArgs();

  header('精度修正脚本');
  kv('实体 ID', `${args.entityId}`);
  kv('节点', args.wsUrl);
  kv('Dry Run', args.dryRun ? '是 (仅查询不提交)' : '否');

  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });

  const owner = args.ownerMnemonic.startsWith('//')
    ? keyring.addFromUri(args.ownerMnemonic)
    : keyring.addFromMnemonic(args.ownerMnemonic);

  const sudoSigner = args.sudoMnemonic.startsWith('//')
    ? keyring.addFromUri(args.sudoMnemonic)
    : keyring.addFromMnemonic(args.sudoMnemonic);

  kv('Entity Owner', owner.address);
  kv('Sudo 账户', sudoSigner.address);

  process.env.WS_URL = args.wsUrl;
  const api = await connectApi(args.wsUrl);

  try {
    const cc = (api.query as any).commissionCore;
    const ccTx = (api.tx as any).commissionCore;
    const nexMarketQuery = (api.query as any).nexMarket;
    const nexMarketTx = (api.tx as any).nexMarket;
    const sudoTx = (api.tx as any).sudo;

    // ================================================================
    // 1. 读取链上当前状态
    // ================================================================
    header('当前链上状态');

    // RepurchaseConfig
    const rcRaw = await cc.repurchaseConfigs(args.entityId);
    if (rcRaw.isNone) {
      console.log('  RepurchaseConfig: 未设置 — 无需修正');
      return;
    }
    const rc = rcRaw.unwrap();
    const oldMinPackage = Number(rc.minPackageUsdt?.toString() ?? rc.min_package_usdt?.toString() ?? '0');
    const oldMaxShopping = Number(rc.maxShoppingBalanceUsdt?.toString() ?? rc.max_shopping_balance_usdt?.toString() ?? '0');
    const oldEnforced = rc.enforced?.isTrue ?? rc.enforced?.toString() === 'true';
    const oldAutoOrder =
      rc.autoOrder?.isTrue ??
      rc.auto_order?.isTrue ??
      ((rc.autoOrder?.toString() === 'true') || (rc.auto_order?.toString() === 'true'));
    const oldProductId = Number(rc.defaultProductId?.toString() ?? rc.default_product_id?.toString() ?? '0');
    const oldTtl = Number(rc.shoppingBalanceTtlBlocks?.toString() ?? rc.shopping_balance_ttl_blocks?.toString() ?? '0');

    kv('min_package_usdt (原值)', `${oldMinPackage} (= ${oldMinPackage / USDT_PRECISION} USDT @ 10^6)`);
    kv('max_shopping_balance_usdt (原值)', `${oldMaxShopping} (= ${oldMaxShopping / USDT_PRECISION} USDT @ 10^6)`);
    kv('enforced', `${oldEnforced}`);
    kv('auto_order', `${oldAutoOrder}`);
    kv('default_product_id', `${oldProductId}`);
    kv('shopping_balance_ttl_blocks', `${oldTtl}`);

    // NEX 价格
    const lastTradePriceRaw = await nexMarketQuery.lastTradePrice();
    const lastTradePrice = Number(lastTradePriceRaw.toString());
    kv('LastTradePrice (原值)', `${lastTradePrice} (= ${lastTradePrice / USDT_PRECISION} USDT @ 10^6)`);

    const ppRaw = await nexMarketQuery.priceProtectionStore();
    const pp = ppRaw.toJSON?.() ?? ppRaw;
    const oldInitialPrice = Number(pp?.initialPrice ?? pp?.initial_price ?? 0);
    kv('initial_price (原值)', `${oldInitialPrice} (= ${oldInitialPrice / USDT_PRECISION} USDT @ 10^6)`);

    // ================================================================
    // 2. 判断是否需要修正 (值 < 10^6 说明缺了精度)
    // ================================================================
    header('精度判断');

    // 判断逻辑: 如果原值 < USDT_PRECISION (10^6)，说明设置时漏乘了精度
    const needFixMinPackage = oldMinPackage > 0 && oldMinPackage < USDT_PRECISION;
    const needFixMaxShopping = oldMaxShopping > 0 && oldMaxShopping < USDT_PRECISION;
    const needFixInitialPrice = oldInitialPrice > 0 && oldInitialPrice < USDT_PRECISION;
    const needFixLastTradePrice = lastTradePrice > 0 && lastTradePrice < USDT_PRECISION;

    kv('min_package_usdt 需修正?', needFixMinPackage ? `是 (${oldMinPackage} → ${oldMinPackage * USDT_PRECISION})` : '否');
    kv('max_shopping_balance_usdt 需修正?', needFixMaxShopping ? `是 (${oldMaxShopping} → ${oldMaxShopping * USDT_PRECISION})` : '否');
    kv('initial_price 需修正?', needFixInitialPrice ? `是 (${oldInitialPrice} → ${oldInitialPrice * USDT_PRECISION})` : '否');
    kv('LastTradePrice 需修正?', needFixLastTradePrice ? '是 (通过 setInitialPrice 间接更新)' : '否');

    if (!needFixMinPackage && !needFixMaxShopping && !needFixInitialPrice && !needFixLastTradePrice) {
      console.log('\n  所有值精度正常，无需修正。');
      return;
    }

    // ================================================================
    // 3. 计算修正值
    // ================================================================
    const newMinPackage = args.minPackageUsdt > 0
      ? args.minPackageUsdt * USDT_PRECISION
      : (needFixMinPackage ? oldMinPackage * USDT_PRECISION : oldMinPackage);

    const newMaxShopping = args.maxShoppingUsdt > 0
      ? args.maxShoppingUsdt * USDT_PRECISION
      : (needFixMaxShopping ? oldMaxShopping * USDT_PRECISION : oldMaxShopping);

    const newInitialPrice = args.nexPriceUsdt > 0
      ? Math.round(args.nexPriceUsdt * USDT_PRECISION)
      : (needFixInitialPrice ? oldInitialPrice * USDT_PRECISION : oldInitialPrice);

    header('修正计划');
    if (needFixMinPackage || needFixMaxShopping) {
      console.log('  [RepurchaseConfig]');
      kv('  min_package_usdt', `${oldMinPackage} → ${newMinPackage} (${newMinPackage / USDT_PRECISION} USDT)`);
      kv('  max_shopping_balance_usdt', `${oldMaxShopping} → ${newMaxShopping} (${newMaxShopping / USDT_PRECISION} USDT)`);
      kv('  enforced', `${oldEnforced} (不变)`);
      kv('  auto_order', `${oldAutoOrder} (不变)`);
      kv('  default_product_id', `${oldProductId} (不变)`);
      kv('  shopping_balance_ttl', `${oldTtl} (不变)`);
    }
    if (needFixInitialPrice || needFixLastTradePrice) {
      console.log('  [NEX 初始价格]');
      kv('  initial_price', `${oldInitialPrice} → ${newInitialPrice} (${newInitialPrice / USDT_PRECISION} USDT)`);
    }

    if (args.dryRun) {
      console.log('\n  [Dry Run] 仅展示修正计划，不提交交易。');
      return;
    }

    // ================================================================
    // 4. 提交修正交易
    // ================================================================

    // 4a. 修正 RepurchaseConfig (Entity Owner)
    if (needFixMinPackage || needFixMaxShopping) {
      header('Step 1: 修正 RepurchaseConfig');
      console.log('  签名者: Entity Owner');

      const newConfig = {
        minPackageUsdt: newMinPackage,
        enforced: oldEnforced,
        autoOrder: oldAutoOrder,
        defaultProductId: oldProductId,
        shoppingBalanceTtlBlocks: oldTtl,
        maxShoppingBalanceUsdt: newMaxShopping,
      };
      console.log('  提交参数:', JSON.stringify(newConfig, null, 2));

      const receipt = await submitTx(
        api,
        ccTx.setRepurchaseConfig(args.entityId, newConfig),
        owner,
        '修正 RepurchaseConfig 精度',
      );

      if (!receipt.success) {
        console.log(`  [失败] ${receipt.error}`);
        process.exit(1);
      }
      console.log(`  [成功] tx: ${receipt.txHash}`);
    }

    // 4b. 修正 NEX 初始价格 (需要 Sudo)
    if (needFixInitialPrice || needFixLastTradePrice) {
      header('Step 2: 修正 NEX 初始价格 (via Sudo)');
      console.log(`  签名者: Sudo (${sudoSigner.address})`);
      console.log(`  新价格: ${newInitialPrice} (${newInitialPrice / USDT_PRECISION} USDT/NEX)`);

      // sudo.sudo(nexMarket.setInitialPrice(newInitialPrice))
      const innerCall = nexMarketTx.setInitialPrice(newInitialPrice);
      const sudoCall = sudoTx.sudo(innerCall);

      const receipt = await submitTx(api, sudoCall, sudoSigner, '修正 NEX 初始价格');

      if (!receipt.success) {
        console.log(`  [失败] ${receipt.error}`);
        // 如果 sudo 失败，尝试显示具体原因
        const sudoErr = receipt.events.find(
          (e: any) => e.section === 'sudo' && e.method === 'Sudid',
        );
        if (sudoErr) {
          console.log(`  Sudid 事件: ${JSON.stringify(sudoErr.data)}`);
        }
        process.exit(1);
      }
      console.log(`  [成功] tx: ${receipt.txHash}`);
    }

    // ================================================================
    // 5. 验证修正结果
    // ================================================================
    header('修正后验证');

    const rcAfter = (await cc.repurchaseConfigs(args.entityId)).unwrap();
    const newMinP = Number(rcAfter.minPackageUsdt?.toString() ?? rcAfter.min_package_usdt?.toString() ?? '0');
    const newMaxS = Number(rcAfter.maxShoppingBalanceUsdt?.toString() ?? rcAfter.max_shopping_balance_usdt?.toString() ?? '0');
    kv('min_package_usdt', `${newMinP} (${newMinP / USDT_PRECISION} USDT)`);
    kv('max_shopping_balance_usdt', `${newMaxS} (${newMaxS / USDT_PRECISION} USDT)`);

    const ltpAfter = Number((await nexMarketQuery.lastTradePrice()).toString());
    kv('LastTradePrice', `${ltpAfter} (${ltpAfter / USDT_PRECISION} USDT)`);

    const ppAfter = (await nexMarketQuery.priceProtectionStore()).toJSON?.() ?? {};
    const ipAfter = Number(ppAfter?.initialPrice ?? ppAfter?.initial_price ?? 0);
    kv('initial_price', `${ipAfter} (${ipAfter / USDT_PRECISION} USDT)`);

    // 重新计算购物余额折算
    const loyalty = (api.query as any).entityLoyalty;
    if (loyalty?.memberShoppingBalance) {
      const bal = BigInt((await loyalty.memberShoppingBalance(args.entityId, owner.address)).toString());
      const rate = BigInt(ltpAfter);
      const balUsdt = (bal * rate) / 1_000_000_000_000n;
      console.log();
      kv('购物余额', `${Number(bal) / 1e12} NEX`);
      kv('折算 USDT', `${Number(balUsdt) / 1e6} USDT`);
      kv('阈值', `${newMaxS / USDT_PRECISION} USDT`);
      kv('是否通过阈值检查?', `${Number(balUsdt) <= newMaxS ? '是' : '否'}`);
    }

    console.log(`\n${ln('═')}`);
    console.log(`  修正完成!`);
    console.log(ln('═') + '\n');

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误:', err.message ?? err);
  process.exit(1);
});
