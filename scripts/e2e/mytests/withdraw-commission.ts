#!/usr/bin/env tsx
/**
 * 会员佣金提现脚本
 *
 * 用法:
 *   npx tsx withdraw-commission.ts --mnemonic "助记词..." --entity 100000 [--amount 100] [--token] [--ws wss://rpc.nexusmall.net]
 *
 * 参数:
 *   --mnemonic, -m   会员助记词 (必填)
 *   --entity,  -e    实体 ID (必填)
 *   --amount,  -a    提现金额 (NEX/Token)，不填则全额提现
 *   --token,   -t    提现 Token 佣金 (默认提现 NEX 佣金)
 *   --rate,    -r    请求复购比例 (bps, 0-10000)，不填则使用默认配置
 *   --ws              WebSocket 地址 (默认 wss://rpc.nexusmall.net)
 *   --help,    -h    显示帮助
 */

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { connectApi, disconnectApi, submitTx } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { readFreeBalance } from '../framework/accounts.js';
import { formatNex, asBigInt, NEX_PLANCK } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

/* ------------------------------------------------------------------ */
/*  命令行参数解析                                                      */
/* ------------------------------------------------------------------ */

interface CliArgs {
  mnemonic: string;
  entityId: number;
  amount: bigint | null;
  isToken: boolean;
  repurchaseRate: number | null;
  wsUrl: string;
}

function printHelp(): void {
  console.log(`
会员佣金提现脚本

用法:
  npx tsx withdraw-commission.ts --mnemonic "助记词..." --entity 100000 [选项]

必填参数:
  --mnemonic, -m   会员助记词 (12/24 个单词)
  --entity,  -e    实体 ID

可选参数:
  --amount,  -a    提现金额 (NEX/Token 单位)，不填则全额提现
  --token,   -t    提现 Token 佣金 (默认提现 NEX 佣金)
  --rate,    -r    请求复购比例 (bps, 0-10000)
  --ws              WebSocket 地址 (默认 wss://rpc.nexusmall.net)
  --help,    -h    显示此帮助信息

示例:
  # 全额提现 NEX 佣金
  npx tsx withdraw-commission.ts -m "word1 word2 ... word12" -e 100000

  # 提现 50 NEX 佣金
  npx tsx withdraw-commission.ts -m "word1 word2 ... word12" -e 100000 -a 50

  # 提现 Token 佣金
  npx tsx withdraw-commission.ts -m "word1 word2 ... word12" -e 100000 --token

  # 指定复购比例 30% (3000 bps)
  npx tsx withdraw-commission.ts -m "word1 word2 ... word12" -e 100000 -r 3000
`);
}

function parseArgs(): CliArgs {
  const argv = process.argv.slice(2);

  if (argv.includes('--help') || argv.includes('-h') || argv.length === 0) {
    printHelp();
    process.exit(0);
  }

  let mnemonic = '';
  let entityId = NaN;
  let amount: bigint | null = null;
  let isToken = false;
  let repurchaseRate: number | null = null;
  let wsUrl = 'wss://rpc.nexusmall.net';

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    switch (arg) {
      case '--mnemonic':
      case '-m':
        mnemonic = argv[++i] ?? '';
        break;
      case '--entity':
      case '-e':
        entityId = Number(argv[++i]);
        break;
      case '--amount':
      case '-a': {
        const val = Number(argv[++i]);
        if (isNaN(val) || val <= 0) {
          console.error('错误: --amount 必须是正数');
          process.exit(1);
        }
        amount = BigInt(Math.round(val * 1e12));
        break;
      }
      case '--token':
      case '-t':
        isToken = true;
        break;
      case '--rate':
      case '-r': {
        const r = Number(argv[++i]);
        if (isNaN(r) || r < 0 || r > 10000) {
          console.error('错误: --rate 必须在 0-10000 之间');
          process.exit(1);
        }
        repurchaseRate = r;
        break;
      }
      case '--ws':
        wsUrl = argv[++i] ?? wsUrl;
        break;
      default:
        console.error(`未知参数: ${arg}`);
        printHelp();
        process.exit(1);
    }
  }

  if (!mnemonic) {
    console.error('错误: 必须提供 --mnemonic 参数');
    process.exit(1);
  }
  if (isNaN(entityId) || entityId <= 0) {
    console.error('错误: 必须提供有效的 --entity 参数');
    process.exit(1);
  }

  return { mnemonic, entityId, amount, isToken, repurchaseRate, wsUrl };
}

/* ------------------------------------------------------------------ */
/*  日志工具                                                            */
/* ------------------------------------------------------------------ */

function ln(char = '─', len = 68): string { return char.repeat(len); }

function header(title: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${title}`);
  console.log(ln('═'));
}

function kv(label: string, value: string): void {
  console.log(`  ${label.padEnd(20)} ${value}`);
}

function formatToken(raw: bigint): string {
  return `${(Number(raw) / 1e12).toLocaleString()} Token`;
}

function formatAmount(raw: bigint, isToken: boolean): string {
  return isToken ? formatToken(raw) : formatNex(raw);
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

/* ------------------------------------------------------------------ */
/*  主流程                                                              */
/* ------------------------------------------------------------------ */

async function main(): Promise<void> {
  const args = parseArgs();
  const currencyLabel = args.isToken ? 'Token' : 'NEX';

  header('会员佣金提现');
  kv('模式', `${currencyLabel} 佣金提现`);
  kv('实体 ID', `${args.entityId}`);
  kv('节点', args.wsUrl);
  if (args.amount !== null) {
    kv('指定金额', formatAmount(args.amount, args.isToken));
  } else {
    kv('提现金额', '全额提现');
  }
  if (args.repurchaseRate !== null) {
    kv('请求复购比例', `${args.repurchaseRate} bps (${(args.repurchaseRate / 100).toFixed(1)}%)`);
  }

  // 1. 初始化密钥
  console.log(`\n  正在初始化密钥...`);
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const signer = keyring.addFromMnemonic(args.mnemonic);
  kv('会员地址', signer.address);
  kv('地址(缩写)', shortAddr(signer.address));

  // 2. 连接节点
  console.log(`\n  正在连接节点...`);
  process.env.WS_URL = args.wsUrl;
  const api = await connectApi(args.wsUrl);

  try {
    const spec = `${api.runtimeVersion.specName} v${api.runtimeVersion.specVersion}`;
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();
    kv('链规格', spec);
    kv('当前区块', `#${currentBlock}`);

    const cc = (api.query as any).commissionCore;
    const tx = (api.tx as any).commissionCore;

    // 3. 查询钱包余额
    header('账户信息');
    const walletBalance = await readFreeBalance(api, signer.address);
    kv('钱包余额', formatNex(walletBalance));

    // 4. 查询佣金统计 (提现前)
    header(`${currencyLabel} 佣金统计 (提现前)`);

    const statsQuery = args.isToken
      ? cc.memberTokenCommissionStats
      : cc.memberCommissionStats;

    const rawStats = codecToJson<Record<string, unknown>>(
      await statsQuery(args.entityId, signer.address),
    );

    const earned = asBigInt(readObjectField(rawStats, 'totalEarned', 'total_earned') ?? 0);
    const pending = asBigInt(readObjectField(rawStats, 'pending') ?? 0);
    const withdrawn = asBigInt(readObjectField(rawStats, 'withdrawn') ?? 0);
    const repurchased = asBigInt(readObjectField(rawStats, 'repurchased') ?? 0);
    const orderCount = coerceNumber(readObjectField(rawStats, 'orderCount', 'order_count')) ?? 0;

    kv('累计收入', formatAmount(earned, args.isToken));
    kv('待提现', formatAmount(pending, args.isToken));
    kv('已提现', formatAmount(withdrawn, args.isToken));
    kv('已复购', formatAmount(repurchased, args.isToken));
    kv('佣金订单数', `${orderCount}`);

    // 5. 检查是否有可提佣金
    if (pending === 0n) {
      console.log(`\n  [!!] 无待提现佣金，退出`);
      return;
    }

    const withdrawAmount = args.amount !== null ? args.amount : null;
    if (withdrawAmount !== null && withdrawAmount > pending) {
      console.log(`\n  [!!] 指定金额 ${formatAmount(withdrawAmount, args.isToken)} 超过待提现 ${formatAmount(pending, args.isToken)}，退出`);
      return;
    }

    const actualAmount = withdrawAmount ?? pending;
    console.log(`\n  本次提现金额: ${formatAmount(actualAmount, args.isToken)}`);

    // 6. 构建并提交提现交易
    header('提交提现交易');
    console.log(`  正在签名并广播交易...`);

    const withdrawExtrinsic = args.isToken
      ? tx.withdrawTokenCommission(
          args.entityId,
          withdrawAmount?.toString() ?? null,
          args.repurchaseRate,
        )
      : tx.withdrawCommission(
          args.entityId,
          withdrawAmount?.toString() ?? null,
          args.repurchaseRate,
        );

    const receipt = await submitTx(api, withdrawExtrinsic, signer, '佣金提现');

    if (!receipt.success) {
      console.log(`\n  [失败] 交易失败!`);
      kv('交易哈希', receipt.txHash);
      kv('错误信息', receipt.error ?? '未知错误');
      process.exit(1);
    }

    console.log(`  [成功] 交易已上链!`);
    kv('交易哈希', receipt.txHash);
    kv('区块哈希', receipt.blockHash ?? '');
    kv('交易索引', `${receipt.extrinsicIndex ?? ''}`);

    // 7. 解析提现事件
    const eventName = args.isToken ? 'TokenTieredWithdrawal' : 'TieredWithdrawal';
    const withdrawalEvent = receipt.events.find(
      (e) => e.section === 'commissionCore' && e.method === eventName,
    );

    if (withdrawalEvent) {
      header('提现拆分详情');
      const data = withdrawalEvent.data as any;
      // 事件 data 可能是数组或对象
      let wdAmount: bigint, rpAmount: bigint, bonusAmount: bigint;

      if (Array.isArray(data)) {
        // [entity_id, account, withdrawn_amount, repurchase_amount, bonus_amount]
        wdAmount = asBigInt(data[2]);
        rpAmount = asBigInt(data[3]);
        bonusAmount = asBigInt(data[4]);
      } else {
        wdAmount = asBigInt(readObjectField(data, 'withdrawnAmount', 'withdrawn_amount') ?? 0);
        rpAmount = asBigInt(readObjectField(data, 'repurchaseAmount', 'repurchase_amount') ?? 0);
        bonusAmount = asBigInt(readObjectField(data, 'bonusAmount', 'bonus_amount') ?? 0);
      }

      const total = wdAmount + rpAmount;
      kv('到账金额 (钱包)', formatAmount(wdAmount, args.isToken));
      kv('复购金额 (购物余额)', formatAmount(rpAmount, args.isToken));
      kv('自愿复购加成', formatAmount(bonusAmount, args.isToken));
      kv('提现总额', formatAmount(total, args.isToken));

      if (total > 0n) {
        const wdPct = ((Number(wdAmount) / Number(total)) * 100).toFixed(1);
        const rpPct = ((Number(rpAmount) / Number(total)) * 100).toFixed(1);
        kv('到账比例', `${wdPct}%`);
        kv('复购比例', `${rpPct}%`);
      }
    }

    // 8. 显示其他事件
    const otherEvents = receipt.events.filter(
      (e) => !(e.section === 'commissionCore' && e.method === eventName),
    );
    if (otherEvents.length > 0) {
      console.log(`\n  其他事件:`);
      for (const evt of otherEvents) {
        console.log(`    ${evt.section}.${evt.method}`);
      }
    }

    // 9. 查询提现后统计
    header(`${currencyLabel} 佣金统计 (提现后)`);

    const rawStatsAfter = codecToJson<Record<string, unknown>>(
      await statsQuery(args.entityId, signer.address),
    );

    const earnedAfter = asBigInt(readObjectField(rawStatsAfter, 'totalEarned', 'total_earned') ?? 0);
    const pendingAfter = asBigInt(readObjectField(rawStatsAfter, 'pending') ?? 0);
    const withdrawnAfter = asBigInt(readObjectField(rawStatsAfter, 'withdrawn') ?? 0);
    const repurchasedAfter = asBigInt(readObjectField(rawStatsAfter, 'repurchased') ?? 0);

    kv('累计收入', formatAmount(earnedAfter, args.isToken));
    kv('待提现', formatAmount(pendingAfter, args.isToken));
    kv('已提现', formatAmount(withdrawnAfter, args.isToken));
    kv('已复购', formatAmount(repurchasedAfter, args.isToken));

    // 10. 前后对比
    header('前后对比');
    const pendingDelta = pending - pendingAfter;
    const withdrawnDelta = withdrawnAfter - withdrawn;
    const repurchasedDelta = repurchasedAfter - repurchased;

    kv('待提现减少', formatAmount(pendingDelta, args.isToken));
    kv('已提现增加', formatAmount(withdrawnDelta, args.isToken));
    kv('已复购增加', formatAmount(repurchasedDelta, args.isToken));

    // 对账验证
    if (pendingDelta === withdrawnDelta + repurchasedDelta) {
      console.log(`\n  [通过] 对账验证: 待提减少 = 提现增加 + 复购增加`);
    } else {
      console.log(`\n  [!!] 对账差异: 待提减少(${formatAmount(pendingDelta, args.isToken)}) != 提现增加(${formatAmount(withdrawnDelta, args.isToken)}) + 复购增加(${formatAmount(repurchasedDelta, args.isToken)})`);
    }

    // 11. 查询钱包余额变化
    const walletAfter = await readFreeBalance(api, signer.address);
    const walletDelta = walletAfter - walletBalance;
    kv('钱包余额变化', `${walletDelta >= 0n ? '+' : ''}${formatNex(walletDelta)}`);
    kv('当前钱包余额', formatNex(walletAfter));

    console.log(`\n${ln('═')}`);
    console.log(`  提现完成!`);
    console.log(ln('═') + '\n');

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误:', err.message ?? err);
  process.exit(1);
});
