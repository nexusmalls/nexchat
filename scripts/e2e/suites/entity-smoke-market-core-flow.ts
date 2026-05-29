import { submitTx } from '../framework/api.js';
import { assert, assertTxSuccess } from '../framework/assert.js';
import { readObjectField } from '../framework/codec.js';
import { TestSuite } from '../framework/types.js';
import { bytes } from './helpers.js';
import {
  VALID_TRON_ADDRESSES,
  findRecentMakerOrder,
  marketFieldContains,
  readMarketOrder,
  readSafeMarketPrices,
  readUserOrders,
} from './market-helpers.js';

const ORDER_AMOUNT = '1000';

/**
 * 实体市场核心冒烟流：创建代币与市场配置，挂卖单并完成撮合。
 */
export const entitySmokeMarketCoreFlowSuite: TestSuite = {
  id: 'entity-smoke-market-core-flow',
  title: 'Entity smoke / market core flow',
  description: 'Create an entity token + market configuration, place a sell order, and have another actor take it through the entity market.',
  tags: ['entity', 'market', 'smoke'],
  async run(ctx) {
    const owner = ctx.actors.ferdie;
    const seller = ctx.actors.bob;
    const buyer = ctx.actors.charlie;
    const tx = ctx.api.tx as any;

    await ctx.step('market actors are funded', async () => {
      await ctx.ensureFundsFor(['ferdie', 'bob', 'charlie'], 25_000);
    });

    const setup = await ctx.step('create an entity, create token, mint balances, and configure market', async () => {
      const createReceipt = await submitTx(
        ctx.api,
        tx.entityRegistry.createEntity(bytes(`market-smoke-${Date.now()}`), null, null, null),
        owner,
        'create market entity',
      );
      assertTxSuccess(createReceipt, 'createEntity should succeed');

      const entityIds = await ((ctx.api.query as any).entityRegistry.userEntities?.(owner.address)
        ?? (ctx.api.query as any).entityRegistry.userEntity(owner.address));
      const ids = (entityIds.toJSON() as unknown[]).map((value) => Number(value));
      const entityId = ids[ids.length - 1];
      assert(entityId > 0, 'new market entity id should exist');

      let receipt = await submitTx(
        ctx.api,
        tx.entityToken.createShopToken(entityId, bytes(`market-token-${entityId}`), bytes(`MT${entityId}`), 0, 0, 0),
        owner,
        'create market token',
      );
      assertTxSuccess(receipt, 'createShopToken should succeed');

      receipt = await submitTx(ctx.api, tx.entityToken.mintTokens(entityId, seller.address, '1000000'), owner, 'mint seller tokens');
      assertTxSuccess(receipt, 'mintTokens seller should succeed');

      receipt = await submitTx(ctx.api, tx.entityMarket.configureMarket(entityId, true, 1, 100), owner, 'configure market');
      assertTxSuccess(receipt, 'configureMarket should succeed');

      receipt = await submitTx(ctx.api, tx.entityMarket.setInitialPrice(entityId, 100_000), owner, 'set initial market price');
      assertTxSuccess(receipt, 'setInitialPrice should succeed');

      return { entityId };
    });

    const prices = await ctx.step('derive safe entity market prices', async () => {
      const basePrice = await ctx.readMarketPrice();
      const prices = await readSafeMarketPrices(ctx.api, basePrice);
      ctx.note(`basePrice=${basePrice} sellPrice=${prices.sellPrice}`);
      return prices;
    });

    const orderId = await ctx.step('seller places an entity market sell order', async () => {
      const beforeNextOrderId = Number((await (ctx.api.query as any).entityMarket.nextOrderId(setup.entityId)).toString());
      const beforeOrders = await readUserOrders(ctx.api, seller.address);

      const receipt = await submitTx(
        ctx.api,
        tx.entityMarket.placeSellOrder(setup.entityId, ORDER_AMOUNT, prices.sellPrice, VALID_TRON_ADDRESSES.seller, null),
        seller,
        'place entity market sell order',
      );
      assertTxSuccess(receipt, 'entityMarket.placeSellOrder should succeed');

      const afterNextOrderId = Number((await (ctx.api.query as any).entityMarket.nextOrderId(setup.entityId)).toString());
      const orderId = await findRecentMakerOrder(ctx.api, seller.address, 'sell', beforeNextOrderId, afterNextOrderId);
      assert(orderId != null, 'should find the new entity market sell order');

      const afterOrders = await readUserOrders(ctx.api, seller.address);
      ctx.note(`sellerOrdersBefore=${beforeOrders.length} sellerOrdersAfter=${afterOrders.length}`);
      return orderId;
    });

    await ctx.step('buyer takes the order and resulting market order is queryable', async () => {
      const receipt = await submitTx(
        ctx.api,
        tx.entityMarket.takeOrder(setup.entityId, orderId, null),
        buyer,
        'take entity market sell order',
      );
      assertTxSuccess(receipt, 'entityMarket.takeOrder should succeed');

      const order = await readMarketOrder(ctx.api, orderId);
      const status = String(readObjectField(order.human, 'status') ?? readObjectField(order.json, 'status') ?? '');
      assert(
        marketFieldContains(order, 'status', 'filled') || marketFieldContains(order, 'status', 'partial') || status.length > 0,
        'market order should remain queryable with a non-empty status after takeOrder',
      );
    });
  },
};
