#!/usr/bin/env tsx
/**
 * 直推会员购买验证脚本 / Direct Referral Purchase Validation Script
 *
 * 目标:
 *   验证下级直推会员购买后：
 *   1. 推荐关系保持正确
 *   2. 买家订单数增加
 *   3. 买家会员消费/激活等状态被更新
 *
 * 用法:
 *   node --import tsx mytests/test-direct-referral-purchase.ts [entityId] [productId]
 *
 * 环境变量:
 *   WS_URL        — WebSocket endpoint (default: ws://127.0.0.1:9944)
 *   ENTITY_ID     — 实体 ID（可替代第一个参数）
 *   PRODUCT_ID    — 商品 ID（可替代第二个参数）
 *   REFERRER_ROLE — 推荐人测试账户角色，默认 bob
 *   BUYER_ROLE    — 下级买家测试账户角色，默认 charlie
 */

process.env.WS_URL ??= 'ws://127.0.0.1:9944';

import { connectApi, disconnectApi, submitTx, type TxReceipt } from '../framework/api.js';
import { assert, assertEqual, assertTxSuccess } from '../framework/assert.js';
import {
  getDevActors,
  ensureNamedActorBalance,
  getSelectedActorsFilePath,
  readFreeBalance,
} from '../framework/accounts.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex } from '../framework/units.js';
import type { DevActors } from '../framework/types.js';

type MemberSnapshot = {
  exists: boolean;
  referrer: string | null;
  directReferrals: number;
  indirectReferrals: number;
  teamSize: number;
  totalSpent: bigint;
  upgradeEligibleSpent: bigint;
  lastActiveAt: number;
  activated: boolean;
  customLevelId: number;
  effectiveLevelId: number;
  orderCount: number;
};

type ProductInfo = {
  shopId: number;
  entityId: number;
  seller: string;
  status: string;
  visibility: string;
  category: string;
  minOrderQuantity: number;
  maxOrderQuantity: number | null;
};

const ENTITY_ID = Number(process.argv[2] ?? process.env.ENTITY_ID ?? '100000');
const PRODUCT_ID = Number(process.argv[3] ?? process.env.PRODUCT_ID ?? '1');
const REFERRER_ROLE = (process.env.REFERRER_ROLE ?? 'bob').trim();
const BUYER_ROLE = (process.env.BUYER_ROLE ?? 'charlie').trim();

function asBigInt(value: unknown): bigint {
  if (typeof value === 'bigint') return value;
  if (typeof value === 'number') return BigInt(value);
  if (typeof value === 'string') {
    const cleaned = value.replace(/,/g, '').trim();
    return cleaned ? BigInt(cleaned) : 0n;
  }
  if (value && typeof (value as any).toString === 'function') {
    try {
      return BigInt((value as any).toString());
    } catch {
      return 0n;
    }
  }
  return 0n;
}

function ln(char = '─', len = 76): string {
  return char.repeat(len);
}

function header(zh: string, en: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${zh}  |  ${en}`);
  console.log(ln('═'));
}

function subHeader(zh: string, en: string): void {
  console.log(`\n  ${ln('─', 66)}`);
  console.log(`  ${zh}  |  ${en}`);
  console.log(`  ${ln('─', 66)}`);
}

function kv(zh: string, en: string, value: string): void {
  console.log(`  ${zh} / ${en}:  ${value}`);
}

function requireActor(actors: DevActors, role: string) {
  const actor = actors[role];
  assert(actor, `缺少测试账户角色 / Missing test actor role: ${role}`);
  return actor;
}

function extractOrderId(receipt: TxReceipt): number {
  const event = receipt.events.find((item) => item.section === 'entityTransaction' && item.method === 'OrderCreated');
  assert(event, '缺少 entityTransaction.OrderCreated 事件 / Missing entityTransaction.OrderCreated event');
  const orderId = coerceNumber(readObjectField(event.data, 'orderId', 'order_id'));
  assert(orderId != null, '无法从事件提取 order_id / Cannot extract order_id from event');
  return orderId;
}

async function readMemberSnapshot(api: any, entityId: number, account: string): Promise<MemberSnapshot> {
  const raw = await (api.query as any).entityMember.entityMembers(entityId, account);
  const exists = !(raw as any).isNone;
  const member = exists ? codecToJson<Record<string, unknown>>((raw as any).unwrap()) : null;
  const orderCount = coerceNumber(await (api.query as any).entityMember.memberOrderCount(entityId, account)) ?? 0;

  let effectiveLevelId = 0;
  try {
    const maybeInfo = await (api.call as any).memberTeamApi?.getMemberInfo?.(entityId, account);
    const info = codecToJson<Record<string, unknown> | null>(maybeInfo);
    if (info) {
      effectiveLevelId = coerceNumber(readObjectField(info, 'effectiveLevelId', 'effective_level_id')) ?? 0;
    }
  } catch {
    // runtime api may be unavailable in some environments
  }

  return {
    exists,
    referrer: member ? (readObjectField(member, 'referrer') ? String(readObjectField(member, 'referrer')) : null) : null,
    directReferrals: member ? (coerceNumber(readObjectField(member, 'directReferrals', 'direct_referrals')) ?? 0) : 0,
    indirectReferrals: member ? (coerceNumber(readObjectField(member, 'indirectReferrals', 'indirect_referrals')) ?? 0) : 0,
    teamSize: member ? (coerceNumber(readObjectField(member, 'teamSize', 'team_size')) ?? 0) : 0,
    totalSpent: member ? asBigInt(readObjectField(member, 'totalSpent', 'total_spent') ?? 0) : 0n,
    upgradeEligibleSpent: member ? asBigInt(readObjectField(member, 'upgradeEligibleSpent', 'upgrade_eligible_spent') ?? 0) : 0n,
    lastActiveAt: member ? (coerceNumber(readObjectField(member, 'lastActiveAt', 'last_active_at')) ?? 0) : 0,
    activated: member ? Boolean(readObjectField(member, 'activated')) : false,
    customLevelId: member ? (coerceNumber(readObjectField(member, 'customLevelId', 'custom_level_id')) ?? 0) : 0,
    effectiveLevelId,
    orderCount,
  };
}

async function readDirectReferralList(api: any, entityId: number, account: string): Promise<string[]> {
  const raw = await (api.query as any).entityMember.directReferrals(entityId, account);
  return (codecToJson<string[]>(raw) ?? []).map(String);
}

async function readProductInfo(api: any, productId: number): Promise<ProductInfo> {
  const raw = await (api.query as any).entityProduct.products(productId);
  assert(!(raw as any).isNone, `商品不存在 / Product not found: ${productId}`);
  const product = codecToJson<Record<string, unknown>>((raw as any).unwrap());

  return {
    shopId: coerceNumber(readObjectField(product, 'shopId', 'shop_id')) ?? 0,
    entityId: coerceNumber(readObjectField(product, 'entityId', 'entity_id')) ?? 0,
    seller: String(readObjectField(product, 'seller') ?? ''),
    status: String(readObjectField(product, 'status') ?? ''),
    visibility: String(readObjectField(product, 'visibility') ?? ''),
    category: String(readObjectField(product, 'category') ?? ''),
    minOrderQuantity: coerceNumber(readObjectField(product, 'minOrderQuantity', 'min_order_quantity')) ?? 1,
    maxOrderQuantity: coerceNumber(readObjectField(product, 'maxOrderQuantity', 'max_order_quantity')) ?? null,
  };
}

function logReceipt(receipt: TxReceipt): void {
  kv('交易哈希', 'Tx Hash', receipt.txHash);
  kv('是否成功', 'Success', String(receipt.success));
  kv('区块哈希', 'Block Hash', receipt.blockHash ?? 'n/a');
  kv('外部索引', 'Extrinsic Index', String(receipt.extrinsicIndex ?? 'n/a'));
  if (receipt.error) {
    kv('错误', 'Error', receipt.error);
  }
  if (receipt.events.length > 0) {
    console.log('  Events:');
    for (const event of receipt.events) {
      console.log(`    - ${event.section}.${event.method} ${JSON.stringify(event.data)}`);
    }
  }
}

async function main(): Promise<void> {
  assert(Number.isInteger(ENTITY_ID) && ENTITY_ID > 0, `ENTITY_ID 无效 / Invalid ENTITY_ID: ${ENTITY_ID}`);
  assert(Number.isInteger(PRODUCT_ID) && PRODUCT_ID > 0, `PRODUCT_ID 无效 / Invalid PRODUCT_ID: ${PRODUCT_ID}`);
  assert(REFERRER_ROLE !== BUYER_ROLE, '推荐人和买家角色不能相同 / Referrer and buyer roles must differ');

  const api = await connectApi();
  try {
    const actors = await getDevActors();
    const referrer = requireActor(actors, REFERRER_ROLE);
    const buyer = requireActor(actors, BUYER_ROLE);
    await ensureNamedActorBalance(api, actors, [REFERRER_ROLE, BUYER_ROLE], 200);

    header('直推会员购买验证', 'Direct Referral Purchase Validation');
    kv('节点', 'WS URL', process.env.WS_URL ?? 'ws://127.0.0.1:9944');
    kv('账户文件', 'Actors File', getSelectedActorsFilePath() ?? '(unknown)');
    kv('实体 ID', 'Entity ID', String(ENTITY_ID));
    kv('商品 ID', 'Product ID', String(PRODUCT_ID));
    kv('推荐人', 'Referrer', `${REFERRER_ROLE}: ${referrer.address}`);
    kv('买家', 'Buyer', `${BUYER_ROLE}: ${buyer.address}`);

    subHeader('前置校验', 'Preflight Checks');
    const product = await readProductInfo(api, PRODUCT_ID);
    kv('商品店铺', 'Product Shop', String(product.shopId));
    kv('商品实体', 'Product Entity', String(product.entityId));
    kv('商品状态', 'Product Status', product.status);
    kv('商品可见性', 'Product Visibility', product.visibility);
    kv('商品类别', 'Product Category', product.category);

    assertEqual(product.entityId, ENTITY_ID, '商品不属于指定实体 / Product does not belong to target entity');
    assert(product.status.includes('OnSale') || product.status.includes('Active'), '商品未上架 / Product is not on sale');
    assert(product.visibility.includes('MembersOnly'), '该脚本要求 MembersOnly 商品 / This script expects a MembersOnly product');

    const referrerBefore = await readMemberSnapshot(api, ENTITY_ID, referrer.address);
    const buyerBefore = await readMemberSnapshot(api, ENTITY_ID, buyer.address);
    const beforeDirects = await readDirectReferralList(api, ENTITY_ID, referrer.address);
    const buyerBalanceBefore = await readFreeBalance(api, buyer.address);

    subHeader('购买前快照', 'Before Purchase Snapshot');
    kv('推荐人已是会员', 'Referrer Is Member', String(referrerBefore.exists));
    kv('买家已是会员', 'Buyer Is Member', String(buyerBefore.exists));
    kv('买家推荐人', 'Buyer Referrer', buyerBefore.referrer ?? '(none)');
    kv('推荐人直推数', 'Referrer Direct Count', String(referrerBefore.directReferrals));
    kv('推荐人团队人数', 'Referrer Team Size', String(referrerBefore.teamSize));
    kv('买家订单数', 'Buyer Order Count', String(buyerBefore.orderCount));
    kv('买家累计消费', 'Buyer Total Spent', String(buyerBefore.totalSpent));
    kv('买家升级消费', 'Buyer Eligible Spent', String(buyerBefore.upgradeEligibleSpent));
    kv('买家已激活', 'Buyer Activated', String(buyerBefore.activated));
    kv('买家余额', 'Buyer Balance', formatNex(buyerBalanceBefore));

    assert(referrerBefore.exists, '推荐人必须已是会员 / Referrer must already be a member');
    assert(buyerBefore.exists, '买家必须已是会员 / Buyer must already be a member');
    assertEqual(buyerBefore.referrer, referrer.address, '买家未绑定到指定推荐人 / Buyer is not bound to expected referrer');
    assert(beforeDirects.includes(buyer.address), '推荐人直推列表中不包含买家 / Referrer direct list does not include buyer');

    subHeader('提交购买交易', 'Submit Purchase Transaction');
    const tx = (api.tx as any).entityTransaction.placeOrder(
      PRODUCT_ID,
      Math.max(product.minOrderQuantity, 1),
      null,
      null,
      null,
      null,
      referrer.address,
      null,
      null,
    );
    const receipt = await submitTx(api, tx, buyer, 'direct-referral-purchase');
    logReceipt(receipt);
    assertTxSuccess(receipt, '直推会员购买交易失败 / Direct referral purchase tx failed');
    const orderId = extractOrderId(receipt);
    kv('订单 ID', 'Order ID', String(orderId));

    subHeader('购买后快照', 'After Purchase Snapshot');
    const referrerAfter = await readMemberSnapshot(api, ENTITY_ID, referrer.address);
    const buyerAfter = await readMemberSnapshot(api, ENTITY_ID, buyer.address);
    const afterDirects = await readDirectReferralList(api, ENTITY_ID, referrer.address);
    const buyerBalanceAfter = await readFreeBalance(api, buyer.address);

    kv('买家推荐人', 'Buyer Referrer', buyerAfter.referrer ?? '(none)');
    kv('推荐人直推数', 'Referrer Direct Count', String(referrerAfter.directReferrals));
    kv('推荐人团队人数', 'Referrer Team Size', String(referrerAfter.teamSize));
    kv('买家订单数', 'Buyer Order Count', String(buyerAfter.orderCount));
    kv('买家累计消费', 'Buyer Total Spent', String(buyerAfter.totalSpent));
    kv('买家升级消费', 'Buyer Eligible Spent', String(buyerAfter.upgradeEligibleSpent));
    kv('买家最后活跃', 'Buyer Last Active', String(buyerAfter.lastActiveAt));
    kv('买家已激活', 'Buyer Activated', String(buyerAfter.activated));
    kv('买家等级', 'Buyer Level', `${buyerAfter.customLevelId} / effective=${buyerAfter.effectiveLevelId}`);
    kv('买家余额', 'Buyer Balance', formatNex(buyerBalanceAfter));

    subHeader('断言', 'Assertions');
    assertEqual(buyerAfter.referrer, referrer.address, '购买后买家推荐关系被破坏 / Buyer referrer changed after purchase');
    assert(afterDirects.includes(buyer.address), '购买后推荐人直推列表丢失买家 / Buyer disappeared from referrer direct list');
    assertEqual(referrerAfter.directReferrals, referrerBefore.directReferrals, '购买不应直接修改推荐人直推计数 / Purchase should not directly change referrer direct count');
    assertEqual(referrerAfter.teamSize, referrerBefore.teamSize, '购买不应直接修改推荐人团队人数 / Purchase should not directly change referrer team size');

    assertEqual(buyerAfter.orderCount, buyerBefore.orderCount + 1, '买家订单数未增加 / Buyer order count did not increase');
    assert(buyerAfter.totalSpent > buyerBefore.totalSpent, '买家累计消费未增加 / Buyer total spent did not increase');
    assert(buyerAfter.upgradeEligibleSpent > buyerBefore.upgradeEligibleSpent, '买家升级消费未增加 / Buyer eligible spent did not increase');
    assert(buyerAfter.lastActiveAt >= buyerBefore.lastActiveAt, '买家最后活跃时间未更新 / Buyer last_active_at did not update');
    assert(buyerAfter.activated, '买家购买后仍未激活 / Buyer is still not activated after purchase');
    assert(buyerBalanceAfter < buyerBalanceBefore, '买家余额未减少，疑似未完成支付 / Buyer balance did not decrease');

    subHeader('结果', 'Result');
    console.log('  OK: 下级直推会员购买后，推荐关系保持正确，且买家会员订单/消费/激活更新已触发。');
    console.log(`  OK: order #${orderId} created successfully.`);
  } finally {
    await disconnectApi(api);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
