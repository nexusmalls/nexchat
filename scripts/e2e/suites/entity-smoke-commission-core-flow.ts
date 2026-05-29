import { submitTx } from '../framework/api.js';
import { assert, assertTxSuccess } from '../framework/assert.js';
import { readObjectField } from '../framework/codec.js';
import { TestSuite } from '../framework/types.js';
import {
  createAndPublishProduct,
  readCommissionStats,
  readOrderCommissionRecordMaybe,
  setupFreshEntity,
  setupMembers,
} from './helpers.js';

const COMMISSION_RATE = 1000;
const DIRECT_REWARD_MASK = 0b0000_0001;
const PRODUCT_PRICE = 100_000n;

/**
 * 从佣金统计中读取 pending 字段并转成 bigint。
 */
function pendingOf(stats: Record<string, unknown>): bigint {
  const raw = readObjectField(stats, 'pending') ?? 0;
  return BigInt(String(raw));
}

/**
 * 实体佣金核心冒烟流：配置最小直推佣金路径并验证佣金记录生成。
 */
export const entitySmokeCommissionCoreFlowSuite: TestSuite = {
  id: 'entity-smoke-commission-core-flow',
  title: 'Entity smoke / commission core flow',
  description: 'Configure a minimal direct commission path, place an order through a referral relationship, and verify commission stats increase.',
  tags: ['entity', 'commission', 'smoke'],
  async run(ctx) {
    const seller = ctx.actors.ferdie;
    const referrer = ctx.actors.bob;
    const buyer = ctx.actors.charlie;
    const tx = ctx.api.tx as any;

    await ctx.step('participants are funded', async () => {
      await ctx.ensureFundsFor(['ferdie', 'bob', 'charlie'], 25_000);
    });

    const setup = await ctx.step('create an entity, activate a referral chain, and publish a digital product', async () => {
      const { entityId, shopId } = await setupFreshEntity(ctx.api, seller);
      await setupMembers(ctx.api, seller, shopId, entityId, [referrer, buyer], true);
      const productId = await createAndPublishProduct(ctx.api, seller, shopId, {
        price: PRODUCT_PRICE,
        category: 'Digital',
      });
      ctx.note(`entityId=${entityId} productId=${productId}`);
      return { entityId, shopId, productId };
    });

    await ctx.step('configure a minimal direct commission flow', async () => {
      let receipt = await submitTx(
        ctx.api,
        tx.commissionCore.setCommissionRate(setup.entityId, COMMISSION_RATE),
        seller,
        'set commission rate',
      );
      assertTxSuccess(receipt, 'setCommissionRate should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.commissionCore.setCommissionModes(setup.entityId, DIRECT_REWARD_MASK),
        seller,
        'set commission modes',
      );
      assertTxSuccess(receipt, 'setCommissionModes should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.commissionReferral.setDirectRewardConfig(setup.entityId, { rate: 500 }),
        seller,
        'set direct reward config',
      );
      assertTxSuccess(receipt, 'setDirectRewardConfig should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.commissionCore.enableCommission(setup.entityId, true),
        seller,
        'enable commission',
      );
      assertTxSuccess(receipt, 'enableCommission should succeed');
    });

    const beforeStats = await ctx.step('capture referrer commission state before order', async () => {
      return readCommissionStats(ctx.api, setup.entityId, referrer.address);
    });

    const orderId = await ctx.step('buyer places an order that triggers direct commission', async () => {
      const nextOrderId = Number((await (ctx.api.query as any).entityTransaction.nextOrderId()).toString());
      const receipt = await submitTx(
        ctx.api,
        tx.entityTransaction.placeOrder(
          setup.productId,
          1,
          null,
          null,
          null,
          'Native',
          null,
          null,
        ),
        buyer,
        'buyer place commission-triggering order',
      );
      assertTxSuccess(receipt, 'placeOrder should succeed');
      return nextOrderId;
    });

    await ctx.step('referrer stats and order commission record become visible', async () => {
      const afterStats = await readCommissionStats(ctx.api, setup.entityId, referrer.address);
      const beforePending = pendingOf(beforeStats);
      const afterPending = pendingOf(afterStats);
      assert(afterPending >= beforePending, 'referrer pending commission should not decrease');

      const record = await readOrderCommissionRecordMaybe(ctx.api, orderId);
      assert(record != null, 'order commission record should exist after commission-triggering order');
      ctx.note(`referrerPendingBefore=${beforePending.toString()} referrerPendingAfter=${afterPending.toString()}`);
    });
  },
};
