/**
 * Shared helpers for Phase 1+ business-flow test suites.
 *
 * Consolidates entity / shop / member / product / order / commission / loyalty
 * read helpers so each suite can stay focused on its own scenario.
 */

import type { ApiPromise } from '@polkadot/api';
import type { KeyringPair } from '@polkadot/keyring/types';
import { submitTx, TxReceipt } from '../framework/api.js';
import { assert, assertTxSuccess } from '../framework/assert.js';
import { codecToHuman, codecToJson, coerceNumber, readObjectField } from '../framework/codec.js';
import { nex } from '../framework/units.js';

/* ------------------------------------------------------------------ */
/*  Codec helpers                                                      */
/* ------------------------------------------------------------------ */

export function bytes(value: string): string {
  return value;
}

/**
 * 将多种链上数值表示统一转成 bigint。
 */
export function asBigInt(value: unknown): bigint {
  if (typeof value === 'bigint') return value;
  if (typeof value === 'number') return BigInt(value);
  if (typeof value === 'string') return BigInt(value.replace(/,/g, '').trim());
  if (value && typeof (value as any).toString === 'function') return BigInt((value as any).toString());
  throw new Error(`Unable to coerce value to bigint: ${String(value)}`);
}

/**
 * 将可选链上数值统一转成 number，不可解析时返回 undefined。
 */
export function asOptionalNumber(value: unknown): number | undefined {
  if (value == null) return undefined;
  if (typeof value === 'object' && value !== null && 'toJSON' in (value as any)) {
    return asOptionalNumber(codecToJson(value));
  }
  return coerceNumber(value);
}

/* ------------------------------------------------------------------ */
/*  Read helpers (entity / shop / member / product / order / commission)*/
/* ------------------------------------------------------------------ */

export async function readEntityIds(api: ApiPromise, address: string): Promise<number[]> {
  const query = (api.query as any).entityRegistry.userEntities
    ?? (api.query as any).entityRegistry.userEntity;
  assert(typeof query === 'function', 'entityRegistry user entity index query should exist');
  const value = await query(address);
  const json = codecToJson<unknown[]>(value);
  return Array.isArray(json) ? json.map((item) => Number(item)) : [];
}

export function readMaxEntitiesPerUser(api: ApiPromise): number {
  const value = (api.consts as any).entityRegistry?.maxEntitiesPerUser;
  return coerceNumber(codecToJson(value)) ?? 1;
}

export async function readEntity(api: ApiPromise, entityId: number): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> }> {
  const value = await (api.query as any).entityRegistry.entities(entityId);
  assert((value as any).isSome, `entity ${entityId} should exist`);
  const entity = (value as any).unwrap();
  return { json: codecToJson(entity), human: codecToHuman(entity) };
}

export function resolvePrimaryShopId(entity: { json: Record<string, unknown>; human: Record<string, unknown> }): number {
  const primaryShopId = coerceNumber(readObjectField(entity.json, 'primaryShopId', 'primary_shop_id'))
    ?? coerceNumber(readObjectField(entity.human, 'primaryShopId', 'primary_shop_id'));
  assert(primaryShopId != null && primaryShopId > 0, 'expected entity to have an auto-created primary shop');
  return primaryShopId;
}

export async function readShop(api: ApiPromise, shopId: number): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> }> {
  const value = await (api.query as any).entityShop.shops(shopId);
  assert((value as any).isSome, `shop ${shopId} should exist`);
  const shop = (value as any).unwrap();
  return { json: codecToJson(shop), human: codecToHuman(shop) };
}

export async function readProduct(api: ApiPromise, productId: number): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> }> {
  const value = await (api.query as any).entityProduct.products(productId);
  assert((value as any).isSome, `product ${productId} should exist`);
  const product = (value as any).unwrap();
  return { json: codecToJson(product), human: codecToHuman(product) };
}

export async function readProductMaybe(api: ApiPromise, productId: number): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> } | null> {
  const value = await (api.query as any).entityProduct.products(productId);
  if ((value as any).isNone) return null;
  const product = (value as any).unwrap();
  return { json: codecToJson(product), human: codecToHuman(product) };
}

export async function readOrder(api: ApiPromise, orderId: number): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> }> {
  const value = await (api.query as any).entityTransaction.orders(orderId);
  assert((value as any).isSome, `order ${orderId} should exist`);
  const order = (value as any).unwrap();
  return { json: codecToJson(order), human: codecToHuman(order) };
}

export async function readMember(api: ApiPromise, entityId: number, address: string): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> }> {
  const value = await (api.query as any).entityMember.entityMembers(entityId, address);
  assert((value as any).isSome, `${address} should be a member of entity ${entityId}`);
  const member = (value as any).unwrap();
  return { json: codecToJson(member), human: codecToHuman(member) };
}

export async function readMemberMaybe(api: ApiPromise, entityId: number, address: string): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> } | null> {
  const value = await (api.query as any).entityMember.entityMembers(entityId, address);
  if ((value as any).isNone) return null;
  const member = (value as any).unwrap();
  return { json: codecToJson(member), human: codecToHuman(member) };
}

export async function readCommissionStats(api: ApiPromise, entityId: number, address: string): Promise<Record<string, unknown>> {
  return codecToJson(await (api.query as any).commissionCore.memberCommissionStats(entityId, address));
}

export function commissionPendingOf(stats: Record<string, unknown>): bigint {
  const raw = readObjectField(stats, 'pending') ?? 0;
  return BigInt(String(raw));
}

export async function readShoppingBalance(api: ApiPromise, entityId: number, address: string): Promise<bigint> {
  return asBigInt(await (api.query as any).entityLoyalty.memberShoppingBalance(entityId, address));
}

export async function readNextOrderId(api: ApiPromise): Promise<number> {
  return Number((await (api.query as any).entityTransaction.nextOrderId()).toString());
}

export async function readNextProductId(api: ApiPromise): Promise<number> {
  return Number((await (api.query as any).entityProduct.nextProductId()).toString());
}

export function decodeStatus(record: { json: Record<string, unknown>; human: Record<string, unknown> }, field: string): string {
  const value = readObjectField(record.human, field) ?? readObjectField(record.json, field);
  return String(value ?? '');
}

export async function readProposal(api: ApiPromise, proposalId: number): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> }> {
  const value = await (api.query as any).entityGovernance.proposals(proposalId);
  assert((value as any).isSome, `proposal ${proposalId} should exist`);
  const proposal = (value as any).unwrap();
  return { json: codecToJson(proposal), human: codecToHuman(proposal) };
}

export async function readNextProposalId(api: ApiPromise): Promise<number> {
  return Number((await (api.query as any).entityGovernance.nextProposalId()).toString());
}

export async function readGovernanceConfig(api: ApiPromise, entityId: number): Promise<Record<string, unknown>> {
  return codecToJson(await (api.query as any).entityGovernance.governanceConfigs(entityId));
}

export function proposalStatusOf(proposal: { json: Record<string, unknown>; human: Record<string, unknown> }): string {
  return String(readObjectField(proposal.human, 'status') ?? readObjectField(proposal.json, 'status') ?? '');
}

export function readProposalTally(proposal: { json: Record<string, unknown>; human: Record<string, unknown> }): { yes: number; no: number; abstain: number } {
  return {
    yes: coerceNumber(readObjectField(proposal.json, 'yesVotes', 'yes_votes') ?? readObjectField(proposal.human, 'yesVotes', 'yes_votes')) ?? 0,
    no: coerceNumber(readObjectField(proposal.json, 'noVotes', 'no_votes') ?? readObjectField(proposal.human, 'noVotes', 'no_votes')) ?? 0,
    abstain: coerceNumber(readObjectField(proposal.json, 'abstainVotes', 'abstain_votes') ?? readObjectField(proposal.human, 'abstainVotes', 'abstain_votes')) ?? 0,
  };
}

export async function readCurrentBlockNumber(api: ApiPromise): Promise<number> {
  return Number((await (api.query as any).system.number()).toString());
}

export async function waitUntilBlock(
  api: ApiPromise,
  target: number,
  attempts: number = 20,
  delayMs: number = 1_500,
): Promise<number> {
  let current = await readCurrentBlockNumber(api);
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    current = await readCurrentBlockNumber(api);
    if (current >= target) {
      return current;
    }
    if (attempt < attempts - 1) {
      await sleep(delayMs);
    }
  }
  throw new Error(`Timed out waiting for block >= ${target}; latest block=${current}`);
}

export async function waitForProposalStatus(
  api: ApiPromise,
  proposalId: number,
  predicate: (status: string) => boolean,
  attempts: number = 12,
  delayMs: number = 1_500,
): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> }> {
  let proposal = await readProposal(api, proposalId);
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    proposal = await readProposal(api, proposalId);
    const status = proposalStatusOf(proposal);
    if (predicate(status)) {
      return proposal;
    }
    if (attempt < attempts - 1) {
      await sleep(delayMs);
    }
  }
  throw new Error(`Timed out waiting for proposal ${proposalId} to reach expected status`);
}

export async function readTokenConfig(api: ApiPromise, entityId: number): Promise<Record<string, unknown>> {
  return codecToJson(await (api.query as any).entityToken.entityTokenConfigs(entityId));
}

export async function readEntityMarketConfig(api: ApiPromise, entityId: number): Promise<Record<string, unknown>> {
  return codecToJson(await (api.query as any).entityMarket.marketConfigs(entityId));
}

export async function readMarketOrderBookEntry(api: ApiPromise, entityId: number, orderId: number): Promise<Record<string, unknown>> {
  const value = await (api.query as any).entityMarket.orders(entityId, orderId);
  assert((value as any).isSome, `entity market order ${orderId} should exist for entity ${entityId}`);
  return codecToJson((value as any).unwrap());
}

export async function readNextEntityMarketOrderId(api: ApiPromise, entityId: number): Promise<number> {
  return Number((await (api.query as any).entityMarket.nextOrderId(entityId)).toString());
}

export async function readEntityTokenBalance(api: ApiPromise, entityId: number, address: string): Promise<bigint> {
  return asBigInt(await (api.query as any).entityToken.tokenBalance(entityId, address));
}

export async function readEntityTokenTotalSupply(api: ApiPromise, entityId: number): Promise<bigint> {
  return asBigInt(await (api.query as any).entityToken.getTotalSupply(entityId));
}

export async function readOrderCommissionRecordMaybe(api: ApiPromise, orderId: number): Promise<Record<string, unknown> | null> {
  const value = await (api.query as any).commissionCore.orderCommissionRecords(orderId);
  if ((value as any).isNone) return null;
  return codecToJson((value as any).unwrap());
}

export async function readVoteRecordMaybe(api: ApiPromise, proposalId: number, address: string): Promise<Record<string, unknown> | null> {
  const value = await (api.query as any).entityGovernance.voteRecords(proposalId, address);
  if ((value as any).isNone) return null;
  return codecToJson((value as any).unwrap());
}

export async function readOrderStatus(api: ApiPromise, orderId: number): Promise<string> {
  return decodeStatus(await readOrder(api, orderId), 'status');
}

/* ------------------------------------------------------------------ */
/*  Event helpers                                                      */
/* ------------------------------------------------------------------ */

export function assertEventIfPresent(
  receipt: { events: Array<{ section: string; method: string }> },
  section: string,
  method: string,
  _message: string,
): boolean {
  if (receipt.events.length === 0) return false;
  return receipt.events.some((e) => e.section === section && e.method === method);
}

/* ------------------------------------------------------------------ */
/*  Setup helpers (create entity + shop + fund + members)              */
/* ------------------------------------------------------------------ */

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * 轮询等待新实体 ID 出现，适配链上状态传播延迟。
 */
export async function waitForNewEntityId(
  api: ApiPromise,
  address: string,
  beforeEntityIds: number[],
  attempts: number = 5,
  delayMs: number = 1_500,
): Promise<{ entityId?: number; ids: number[] }> {
  let lastIds: number[] = beforeEntityIds;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    lastIds = await readEntityIds(api, address);
    const created = lastIds.find((candidate) => !beforeEntityIds.includes(candidate));
    if (created != null) return { entityId: created, ids: lastIds };
    if (attempt < attempts - 1) await sleep(delayMs);
  }
  return { ids: lastIds };
}

/**
 * 创建新实体、解析自动主店铺并为店铺注入初始经营资金。
 * 返回 { entityId, shopId }。
 */
export async function setupFreshEntity(
  api: ApiPromise,
  seller: KeyringPair,
  shopFund: bigint = nex(2_000),
): Promise<{ entityId: number; shopId: number }> {
  const beforeEntityIds = await readEntityIds(api, seller.address);
  const createTx = (api.tx as any).entityRegistry.createEntity(bytes(`e2e-${Date.now()}`), null, null, null);
  const createReceipt = await submitTx(api, createTx, seller, 'create entity');
  assertTxSuccess(createReceipt, 'create entity should succeed');

  const detected = await waitForNewEntityId(api, seller.address, beforeEntityIds);
  assert(detected.entityId != null && detected.entityId > 0, 'should detect newly created entity id');
  const entityId = detected.entityId!;

  const entity = await readEntity(api, entityId);
  const shopId = resolvePrimaryShopId(entity);

  const fundTx = (api.tx as any).entityShop.fundOperating(shopId, shopFund.toString());
  const fundReceipt = await submitTx(api, fundTx, seller, 'fund operating');
  assertTxSuccess(fundReceipt, 'fund operating should succeed');

  return { entityId, shopId };
}

/**
 * 开启注册、批量注册成员并按需构造推荐关系后激活成员。
 */
export async function setupMembers(
  api: ApiPromise,
  seller: KeyringPair,
  shopId: number,
  entityId: number,
  members: KeyringPair[],
  referralChain: boolean = false,
): Promise<void> {
  // open registration
  const policyTx = (api.tx as any).entityMember.setMemberPolicy(shopId, 0);
  const policyReceipt = await submitTx(api, policyTx, seller, 'set member policy open');
  assertTxSuccess(policyReceipt, 'set member policy should succeed');

  // register
  for (let i = 0; i < members.length; i++) {
    const referrer = referralChain && i > 0 ? members[i - 1].address : null;
    const regTx = (api.tx as any).entityMember.registerMember(shopId, referrer);
    const regReceipt = await submitTx(api, regTx, members[i], `register ${members[i].meta.name ?? i}`);
    assertTxSuccess(regReceipt, `register member ${i} should succeed`);
  }

  // activate
  for (const member of members) {
    const actTx = (api.tx as any).entityMember.activateMember(shopId, member.address);
    const actReceipt = await submitTx(api, actTx, seller, `activate ${member.meta.name ?? member.address}`);
    assertTxSuccess(actReceipt, `activate ${member.address} should succeed`);
  }
}

/**
 * 创建并发布商品，返回新商品 ID。
 */
export async function createAndPublishProduct(
  api: ApiPromise,
  seller: KeyringPair,
  shopId: number,
  opts: {
    price?: bigint;
    stock?: number;
    category?: string;
    visibility?: string;
  } = {},
): Promise<number> {
  const price = opts.price ?? nex(100);
  const stock = opts.stock ?? 0;
  const category = opts.category ?? 'Digital';
  const visibility = opts.visibility ?? 'Public';
  const ts = Date.now();

  const nextProductId = await readNextProductId(api);
  const createTx = (api.tx as any).entityProduct.createProduct(
    shopId,
    bytes(`prod-${ts}`),
    bytes(`img-${ts}`),
    bytes(`detail-${ts}`),
    price.toString(),
    0,       // usdt_price
    stock,   // stock (0 = unlimited)
    category,
    0,       // sort_weight
    bytes(''),  // tags_cid
    bytes(''),  // sku_cid
    0,       // min_order_quantity
    0,       // max_order_quantity
    visibility,
  );
  const createReceipt = await submitTx(api, createTx, seller, 'create product');
  assertTxSuccess(createReceipt, 'create product should succeed');

  const publishTx = (api.tx as any).entityProduct.publishProduct(nextProductId);
  const publishReceipt = await submitTx(api, publishTx, seller, 'publish product');
  assertTxSuccess(publishReceipt, 'publish product should succeed');

  return nextProductId;
}
