import { submitTx } from '../framework/api.js';
import { assert, assertEqual, assertTxSuccess } from '../framework/assert.js';
import { coerceNumber, readObjectField } from '../framework/codec.js';
import { TestSuite } from '../framework/types.js';
import { nex } from '../framework/units.js';
import {
  createAndPublishProduct,
  decodeStatus,
  readMemberMaybe,
  readOrder,
  readShoppingBalance,
  setupFreshEntity,
} from './helpers.js';

const PRODUCT_PRICE = nex(25);

/**
 * 实体买家核心冒烟流：下单数字商品并验证订单、会员与购物金状态。
 */
export const entitySmokeBuyerCoreFlowSuite: TestSuite = {
  id: 'entity-smoke-buyer-core-flow',
  title: 'Entity smoke / buyer core flow',
  description: 'Create an entity product, place a buyer digital order, and verify the order auto-completes with member + loyalty state updated.',
  tags: ['entity', 'buyer', 'smoke'],
  async run(ctx) {
    const seller = ctx.actors.ferdie;
    const buyer = ctx.actors.bob;
    const tx = ctx.api.tx as any;

    await ctx.step('buyer and seller are funded', async () => {
      await ctx.ensureFundsFor(['ferdie', 'bob'], 25_000);
    });

    const setup = await ctx.step('create a fresh entity and publish a digital product', async () => {
      const { entityId, shopId } = await setupFreshEntity(ctx.api, seller);
      const policyReceipt = await submitTx(ctx.api, tx.entityMember.setMemberPolicy(shopId, 0), seller, 'open member policy');
      assertTxSuccess(policyReceipt, 'setMemberPolicy should succeed');

      const productId = await createAndPublishProduct(ctx.api, seller, shopId, {
        price: PRODUCT_PRICE,
        category: 'Digital',
        visibility: 'Public',
      });
      ctx.note(`entityId=${entityId} shopId=${shopId} productId=${productId}`);
      return { entityId, shopId, productId };
    });

    const beforeBalance = await ctx.step('capture buyer shopping balance before order', async () => {
      return readShoppingBalance(ctx.api, setup.entityId, buyer.address);
    });

    const orderId = await ctx.step('buyer places a digital order that auto-completes', async () => {
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
        'buyer place digital order',
      );
      assertTxSuccess(receipt, 'placeOrder should succeed');
      return nextOrderId;
    });

    await ctx.step('order auto-completes and buyer becomes a member', async () => {
      const order = await readOrder(ctx.api, orderId);
      const status = decodeStatus(order, 'status');
      assertEqual(status, 'Completed', 'digital order should auto-complete');

      const member = await readMemberMaybe(ctx.api, setup.entityId, buyer.address);
      assert(member != null, 'buyer should become a member after the purchase flow');

      const spent = coerceNumber(readObjectField(member.json, 'totalSpent', 'total_spent')) ?? 0;
      assert(spent > 0, 'buyer totalSpent should increase after purchase');
    });

    await ctx.step('buyer shopping balance and loyalty path remain queryable', async () => {
      const afterBalance = await readShoppingBalance(ctx.api, setup.entityId, buyer.address);
      assert(afterBalance >= beforeBalance, 'buyer shopping balance should be queryable after purchase');
      ctx.note(`shoppingBalanceBefore=${beforeBalance.toString()} shoppingBalanceAfter=${afterBalance.toString()}`);
    });
  },
};
