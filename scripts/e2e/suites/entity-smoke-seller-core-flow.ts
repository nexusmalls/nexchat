import { submitTx } from '../framework/api.js';
import { assertEqual, assertTxSuccess } from '../framework/assert.js';
import { TestSuite } from '../framework/types.js';
import { nex } from '../framework/units.js';
import {
  createAndPublishProduct,
  decodeStatus,
  readOrder,
  setupFreshEntity,
} from './helpers.js';

const PRODUCT_PRICE = nex(30);

/**
 * 实体卖家核心冒烟流：卖家履约实物订单并验证状态流转。
 */
export const entitySmokeSellerCoreFlowSuite: TestSuite = {
  id: 'entity-smoke-seller-core-flow',
  title: 'Entity smoke / seller core flow',
  description: 'Create a physical product, have the buyer place an order, then verify seller fulfilment transitions Paid -> Shipped -> Completed.',
  tags: ['entity', 'seller', 'smoke'],
  async run(ctx) {
    const seller = ctx.actors.eve;
    const buyer = ctx.actors.charlie;
    const tx = ctx.api.tx as any;

    await ctx.step('buyer and seller are funded', async () => {
      await ctx.ensureFundsFor(['eve', 'charlie'], 25_000);
    });

    const setup = await ctx.step('create a fresh entity and publish a physical product', async () => {
      const { entityId, shopId } = await setupFreshEntity(ctx.api, seller);
      const productId = await createAndPublishProduct(ctx.api, seller, shopId, {
        price: PRODUCT_PRICE,
        category: 'Physical',
        visibility: 'Public',
      });
      ctx.note(`entityId=${entityId} shopId=${shopId} productId=${productId}`);
      return { entityId, shopId, productId };
    });

    const orderId = await ctx.step('buyer places a physical order', async () => {
      const nextOrderId = Number((await (ctx.api.query as any).entityTransaction.nextOrderId()).toString());
      const receipt = await submitTx(
        ctx.api,
        tx.entityTransaction.placeOrder(
          setup.productId,
          1,
          'ship-cid-smoke',
          null,
          null,
          'Native',
          null,
          null,
        ),
        buyer,
        'buyer place physical order',
      );
      assertTxSuccess(receipt, 'physical placeOrder should succeed');
      return nextOrderId;
    });

    await ctx.step('seller ships the order', async () => {
      const receipt = await submitTx(
        ctx.api,
        tx.entityTransaction.shipOrder(orderId, 'tracking-smoke', null),
        seller,
        'seller ship order',
      );
      assertTxSuccess(receipt, 'shipOrder should succeed');

      const order = await readOrder(ctx.api, orderId);
      assertEqual(decodeStatus(order, 'status'), 'Shipped', 'order should become shipped after seller fulfilment');
    });

    await ctx.step('buyer confirms receipt and completes the order', async () => {
      const receipt = await submitTx(
        ctx.api,
        tx.entityTransaction.confirmReceipt(orderId),
        buyer,
        'buyer confirm receipt',
      );
      assertTxSuccess(receipt, 'confirmReceipt should succeed');

      const order = await readOrder(ctx.api, orderId);
      assertEqual(decodeStatus(order, 'status'), 'Completed', 'physical order should complete after buyer confirmation');
    });
  },
};
