#!/usr/bin/env tsx
/**
 * 池化奖励领奖脚本
 *
 * 用法:
 *   npx tsx claim-pool-reward.ts --mnemonic "助记词..." --entity 100000 [--query-only] [--ws wss://rpc.nexusmall.net]
 *
 * 参数:
 *   --mnemonic, -m   会员助记词 (必填)
 *   --entity,  -e    实体 ID (必填)
 *   --query-only, -q 仅查询领奖资格和可领金额，不发交易
 *   --ws              WebSocket 地址 (默认 wss://rpc.nexusmall.net)
 *   --help,    -h    显示帮助
 */

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { readFreeBalance } from '../framework/accounts.js';
import { connectApi, disconnectApi, submitTx } from '../framework/api.js';
import { codecToJson, coerceNumber, readObjectField } from '../framework/codec.js';
import { asBigInt, formatNex } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

interface CliArgs {
  mnemonic: string;
  entityId: number;
  queryOnly: boolean;
  wsUrl: string;
}

interface PoolRewardMemberView {
  currentRoundId: bigint;
  claimableNex: bigint;
  claimableToken: bigint;
  alreadyClaimed: boolean;
  roundExpired: boolean;
  lastClaimedRound: bigint;
  effectiveLevel: number;
  isPaused: boolean;
}

function printHelp(): void {
  console.log(`
池化奖励领奖脚本

用法:
  npx tsx claim-pool-reward.ts --mnemonic "助记词..." --entity 100000 [选项]

必填参数:
  --mnemonic, -m   会员助记词 (12/24 个单词)
  --entity,  -e    实体 ID

可选参数:
  --query-only, -q 仅查询资格和可领奖励，不发交易
  --ws              WebSocket 地址 (默认 wss://rpc.nexusmall.net)
  --help,    -h    显示此帮助信息

示例:
  # 查询当前账户本轮可领奖励
  npx tsx claim-pool-reward.ts -m "word1 word2 ... word12" -e 100000 -q

  # 直接执行领奖
  npx tsx claim-pool-reward.ts -m "word1 word2 ... word12" -e 100000
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
  let queryOnly = false;
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
      case '--query-only':
      case '-q':
        queryOnly = true;
        break;
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

  return { mnemonic, entityId, queryOnly, wsUrl };
}

function ln(char = '─', len = 68): string { return char.repeat(len); }

function header(title: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${title}`);
  console.log(ln('═'));
}

function kv(label: string, value: string): void {
  console.log(`  ${label.padEnd(20)} ${value}`);
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

function parseMemberView(raw: Record<string, unknown>): PoolRewardMemberView {
  return {
    currentRoundId: asBigInt(readObjectField(raw, 'currentRoundId', 'current_round_id') ?? 0),
    claimableNex: asBigInt(readObjectField(raw, 'claimableNex', 'claimable_nex') ?? 0),
    claimableToken: asBigInt(readObjectField(raw, 'claimableToken', 'claimable_token') ?? 0),
    alreadyClaimed: Boolean(readObjectField(raw, 'alreadyClaimed', 'already_claimed') ?? false),
    roundExpired: Boolean(readObjectField(raw, 'roundExpired', 'round_expired') ?? false),
    lastClaimedRound: asBigInt(readObjectField(raw, 'lastClaimedRound', 'last_claimed_round') ?? 0),
    effectiveLevel: coerceNumber(readObjectField(raw, 'effectiveLevel', 'effective_level')) ?? 0,
    isPaused: Boolean(readObjectField(raw, 'isPaused', 'is_paused') ?? false),
  };
}

async function queryMemberView(api: any, entityId: number, address: string): Promise<PoolRewardMemberView | null> {
  try {
    const codec = await api.call.poolRewardDetailApi?.getPoolRewardMemberView?.(entityId, address);
    if (!codec) {
      return null;
    }

    const parsed = codecToJson<Record<string, unknown> | null>(codec);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return null;
    }

    return parseMemberView(parsed);
  } catch {
    return null;
  }
}

function printMemberView(view: PoolRewardMemberView): void {
  header('池化奖励状态');
  kv('当前轮次', `#${view.currentRoundId}`);
  kv('有效等级', `Lv${view.effectiveLevel}`);
  kv('可领 NEX', formatNex(view.claimableNex));
  kv('可领 Token', `${view.claimableToken.toString()} planck`);
  kv('本轮已领', view.alreadyClaimed ? '是' : '否');
  kv('轮次已过期', view.roundExpired ? '是' : '否');
  kv('奖池已暂停', view.isPaused ? '是' : '否');
  kv('上次领奖轮次', `#${view.lastClaimedRound}`);
}

async function main(): Promise<void> {
  const args = parseArgs();

  header('池化奖励领奖');
  kv('模式', args.queryOnly ? '仅查询' : '查询并领奖');
  kv('实体 ID', `${args.entityId}`);
  kv('节点', args.wsUrl);

  console.log(`\n  正在初始化密钥...`);
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const signer = keyring.addFromMnemonic(args.mnemonic);
  kv('会员地址', signer.address);
  kv('地址(缩写)', shortAddr(signer.address));

  console.log(`\n  正在连接节点...`);
  process.env.WS_URL = args.wsUrl;
  const api = await connectApi(args.wsUrl);

  try {
    const spec = `${api.runtimeVersion.specName} v${api.runtimeVersion.specVersion}`;
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();
    kv('链规格', spec);
    kv('当前区块', `#${currentBlock}`);

    header('账户信息');
    const walletBefore = await readFreeBalance(api, signer.address);
    kv('钱包余额', formatNex(walletBefore));

    const memberViewBefore = await queryMemberView(api, args.entityId, signer.address);
    if (!memberViewBefore) {
      console.log(`\n  [!!] 无法获取池化奖励会员视图，请确认 runtime API 已注册且该账户具备会员上下文`);
      return;
    }

    printMemberView(memberViewBefore);

    if (args.queryOnly) {
      console.log(`\n${ln('═')}`);
      console.log('  查询完成!');
      console.log(ln('═') + '\n');
      return;
    }

    if (memberViewBefore.isPaused) {
      console.log(`\n  [!!] 奖池当前已暂停，无法领奖`);
      return;
    }
    if (memberViewBefore.roundExpired) {
      console.log(`\n  [!!] 当前轮次已过期，请等待新轮次开始后再试`);
      return;
    }
    if (memberViewBefore.alreadyClaimed) {
      console.log(`\n  [!!] 当前账户本轮已经领过奖励`);
      return;
    }
    if (memberViewBefore.claimableNex === 0n) {
      console.log(`\n  [!!] 当前账户本轮可领 NEX 为 0，无法领奖`);
      return;
    }

    header('提交领奖交易');
    console.log('  正在签名并广播交易...');

    const claimExtrinsic = (api.tx as any).commissionPoolReward.claimPoolReward(args.entityId);
    const receipt = await submitTx(api, claimExtrinsic, signer, '池化奖励领奖');

    if (!receipt.success) {
      console.log(`\n  [失败] 交易失败!`);
      kv('交易哈希', receipt.txHash);
      kv('错误信息', receipt.error ?? '未知错误');
      process.exit(1);
    }

    console.log('  [成功] 交易已上链!');
    kv('交易哈希', receipt.txHash);
    kv('区块哈希', receipt.blockHash ?? '');
    kv('交易索引', `${receipt.extrinsicIndex ?? ''}`);

    const claimEvent = receipt.events.find(
      (e) => e.section === 'commissionPoolReward' && e.method === 'PoolRewardClaimed',
    );

    if (claimEvent) {
      header('领奖事件详情');
      const data = claimEvent.data as any;
      let amount = 0n;
      let tokenAmount = 0n;
      let roundId = 0n;
      let levelId = 0;

      if (Array.isArray(data)) {
        amount = asBigInt(data[2]);
        tokenAmount = asBigInt(data[3]);
        roundId = asBigInt(data[4]);
        levelId = coerceNumber(data[5]) ?? 0;
      } else {
        amount = asBigInt(readObjectField(data, 'amount') ?? 0);
        tokenAmount = asBigInt(readObjectField(data, 'tokenAmount', 'token_amount') ?? 0);
        roundId = asBigInt(readObjectField(data, 'roundId', 'round_id') ?? 0);
        levelId = coerceNumber(readObjectField(data, 'levelId', 'level_id')) ?? 0;
      }

      kv('领取 NEX', formatNex(amount));
      kv('领取 Token', `${tokenAmount.toString()} planck`);
      kv('领奖轮次', `#${roundId}`);
      kv('等级', `Lv${levelId}`);
    }

    const otherEvents = receipt.events.filter(
      (e) => !(e.section === 'commissionPoolReward' && e.method === 'PoolRewardClaimed'),
    );
    if (otherEvents.length > 0) {
      console.log(`\n  其他事件:`);
      for (const evt of otherEvents) {
        console.log(`    ${evt.section}.${evt.method}`);
      }
    }

    const walletAfter = await readFreeBalance(api, signer.address);
    const walletDelta = walletAfter - walletBefore;

    const memberViewAfter = await queryMemberView(api, args.entityId, signer.address);

    header('领奖后状态');
    kv('钱包余额变化', `${walletDelta >= 0n ? '+' : ''}${formatNex(walletDelta)}`);
    kv('当前钱包余额', formatNex(walletAfter));

    if (memberViewAfter) {
      kv('当前轮次', `#${memberViewAfter.currentRoundId}`);
      kv('本轮已领', memberViewAfter.alreadyClaimed ? '是' : '否');
      kv('上次领奖轮次', `#${memberViewAfter.lastClaimedRound}`);
      kv('剩余可领 NEX', formatNex(memberViewAfter.claimableNex));

      const roundMatched = memberViewAfter.lastClaimedRound === memberViewBefore.currentRoundId;
      console.log(`\n  ${memberViewAfter.alreadyClaimed ? '[通过]' : '[!!]'} already_claimed ${memberViewAfter.alreadyClaimed ? '已更新为 true' : '未更新为 true'}`);
      console.log(`  ${roundMatched ? '[通过]' : '[!!]'} last_claimed_round ${roundMatched ? '已更新到本轮' : '未更新到本轮'}`);
    } else {
      console.log(`\n  [!!] 领奖后无法重新查询会员视图，仅完成了交易与余额校验`);
    }

    console.log(`\n${ln('═')}`);
    console.log('  领奖完成!');
    console.log(ln('═') + '\n');
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误:', err.message ?? err);
  process.exit(1);
});
