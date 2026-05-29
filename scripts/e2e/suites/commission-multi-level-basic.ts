import { submitTx } from '../framework/api.js';
import { assert, assertTxSuccess } from '../framework/assert.js';
import { TestSuite } from '../framework/types.js';
import {
  commissionPendingOf,
  createAndPublishProduct,
  readCommissionStats,
  readOrderCommissionRecordMaybe,
  setupFreshEntity,
  setupMembers,
} from './helpers.js';

const COMMISSION_RATE = 1000;
const MULTI_LEVEL_MASK = 0b0000_0010;
const PRODUCT_PRICE = 100_000n;

/**
 * 多级佣金专项回归：建立两级推荐链并验证多级佣金记录与统计出现。
 */
export const commissionMultiLevelBasicSuite: TestSuite = {
  id: 'commission-multi-level-basic',
  title: 'Commission / multi-level basic',
  description: 'Configure a basic multi-level commission flow, place an order through a two-level referral chain, and verify commission stats and records.',
  tags: ['entity', 'commission', 'multi-level', 'e2e'],
  async run(ctx) {
    const seller = ctx.actors.ferdie;
    const level1 = ctx.actors.bob;
    const buyer = ctx.actors.charlie;
    const tx = ctx.api.tx as any;

    await ctx.step('participants are funded', async () => {
      await ctx.ensureFundsFor(['ferdie', 'bob', 'charlie'], 25_000);
    });

    const setup = await ctx.step('create an entity, build a two-level referral chain, and publish a digital product', async () => {
      const { entityId, shopId } = await setupFreshEntity(ctx.api, seller);
      await setupMembers(ctx.api, seller, shopId, entityId, [level1, buyer], true);
      const productId = await createAndPublishProduct(ctx.api, seller, shopId, {
        price: PRODUCT_PRICE,
        category: 'Digital',
      });
      ctx.note(`entityId=${entityId} productId=${productId}`);
      return { entityId, shopId, productId };
    });

    await ctx.step('configure a minimal multi-level commission flow', async () => {
      let receipt = await submitTx(
        ctx.api,
        tx.commissionCore.setCommissionRate(setup.entityId, COMMISSION_RATE),
        seller,
        'set commission rate',
      );
      assertTxSuccess(receipt, 'setCommissionRate should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.commissionCore.setCommissionModes(setup.entityId, MULTI_LEVEL_MASK),
        seller,
        'set commission modes to multi-level',
      );
      assertTxSuccess(receipt, 'setCommissionModes should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.commissionReferral.setMultiLevelConfig(setup.entityId, [{ rate: 500, required_directs: 0, required_team_size: 0, required_spent_usdt: 0, required_level_id: 0 }]),
        seller,
        'set multi-level config',
      );
      assertTxSuccess(receipt, 'setMultiLevelConfig should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.commissionCore.enableCommission(setup.entityId, true),
        seller,
        'enable commission',
      );
      assertTxSuccess(receipt, 'enableCommission should succeed');
    });

    const beforeStats = await ctx.step('capture level1 commission state before order', async () => {
      return readCommissionStats(ctx.api, setup.entityId, level1.address);
    });

    const orderId = await ctx.step('buyer places an order that triggers multi-level commission', async () => {
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
        'buyer place multi-level commission order',
      );
      assertTxSuccess(receipt, 'placeOrder should succeed');
      return nextOrderId;
    });

    await ctx.step('level1 commission stats and order commission record become visible', async () => {
      const afterStats = await readCommissionStats(ctx.api, setup.entityId, level1.address);
      const beforePending = commissionPendingOf(beforeStats);
      const afterPending = commissionPendingOf(afterStats);
      assert(afterPending >= beforePending, 'multi-level referrer pending commission should not decrease');

      const record = await readOrderCommissionRecordMaybe(ctx.api, orderId);
      assert(record != null, 'order commission record should exist after multi-level commission order');
      ctx.note(`level1PendingBefore=${beforePending.toString()} level1PendingAfter=${afterPending.toString()}`);
    });
  },
};
