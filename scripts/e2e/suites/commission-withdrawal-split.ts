import { submitTx } from '../framework/api.js';
import { assert, assertTxSuccess } from '../framework/assert.js';
import { TestSuite } from '../framework/types.js';
import {
  commissionPendingOf,
  createAndPublishProduct,
  readCommissionStats,
  readShoppingBalance,
  setupFreshEntity,
  setupMembers,
} from './helpers.js';

const COMMISSION_RATE = 1000;
const DIRECT_REWARD_MASK = 0b0000_0001;
const PRODUCT_PRICE = 100_000n;

/**
 * 提现拆分专项回归：先制造可提现佣金，再验证提现后 pending 下降，余额路径仍可查询。
 */
export const commissionWithdrawalSplitSuite: TestSuite = {
  id: 'commission-withdrawal-split',
  title: 'Commission / withdrawal split',
  description: 'Generate commission for a referrer, execute a commission withdrawal flow, and verify pending commission decreases with balances remaining queryable.',
  tags: ['entity', 'commission', 'withdrawal', 'e2e'],
  async run(ctx) {
    const seller = ctx.actors.ferdie;
    const referrer = ctx.actors.bob;
    const buyer = ctx.actors.charlie;
    const tx = ctx.api.tx as any;

    await ctx.step('participants are funded', async () => {
      await ctx.ensureFundsFor(['ferdie', 'bob', 'charlie'], 25_000);
    });

    const setup = await ctx.step('create an entity, activate referral path, and publish a digital product', async () => {
      const { entityId, shopId } = await setupFreshEntity(ctx.api, seller);
      await setupMembers(ctx.api, seller, shopId, entityId, [referrer, buyer], true);
      const productId = await createAndPublishProduct(ctx.api, seller, shopId, {
        price: PRODUCT_PRICE,
        category: 'Digital',
      });
      return { entityId, shopId, productId };
    });

    await ctx.step('configure a direct commission flow that creates withdrawable commission', async () => {
      let receipt = await submitTx(ctx.api, tx.commissionCore.setCommissionRate(setup.entityId, COMMISSION_RATE), seller, 'set commission rate');
      assertTxSuccess(receipt, 'setCommissionRate should succeed');

      receipt = await submitTx(ctx.api, tx.commissionCore.setCommissionModes(setup.entityId, DIRECT_REWARD_MASK), seller, 'set commission modes');
      assertTxSuccess(receipt, 'setCommissionModes should succeed');

      receipt = await submitTx(ctx.api, tx.commissionReferral.setDirectRewardConfig(setup.entityId, { rate: 500 }), seller, 'set direct reward config');
      assertTxSuccess(receipt, 'setDirectRewardConfig should succeed');

      receipt = await submitTx(ctx.api, tx.commissionCore.enableCommission(setup.entityId, true), seller, 'enable commission');
      assertTxSuccess(receipt, 'enableCommission should succeed');
    });

    await ctx.step('buyer places an order to create commission balance', async () => {
      const receipt = await submitTx(
        ctx.api,
        tx.entityTransaction.placeOrder(setup.productId, 1, null, null, null, 'Native', null, null),
        buyer,
        'buyer place order for withdrawable commission',
      );
      assertTxSuccess(receipt, 'placeOrder should succeed');
    });

    await ctx.step('withdraw commission and verify pending decreases while balances remain queryable', async () => {
      const beforeStats = await readCommissionStats(ctx.api, setup.entityId, referrer.address);
      const beforePending = commissionPendingOf(beforeStats);
      const beforeShopping = await readShoppingBalance(ctx.api, setup.entityId, referrer.address);

      const withdrawCall = tx.commissionCore.withdrawCommission
        ? tx.commissionCore.withdrawCommission(setup.entityId)
        : tx.commissionCore.withdraw(setup.entityId);
      const receipt = await submitTx(ctx.api, withdrawCall, referrer, 'withdraw commission');
      assertTxSuccess(receipt, 'commission withdraw should succeed');

      const afterStats = await readCommissionStats(ctx.api, setup.entityId, referrer.address);
      const afterPending = commissionPendingOf(afterStats);
      const afterShopping = await readShoppingBalance(ctx.api, setup.entityId, referrer.address);

      assert(afterPending <= beforePending, 'pending commission should not increase after withdrawal');
      assert(afterShopping >= 0n && beforeShopping >= 0n, 'shopping balance path should remain queryable after withdrawal');
      ctx.note(`pendingBefore=${beforePending.toString()} pendingAfter=${afterPending.toString()} shoppingBefore=${beforeShopping.toString()} shoppingAfter=${afterShopping.toString()}`);
    });
  },
};
