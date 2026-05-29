import { submitTx } from '../framework/api.js';
import { assert } from '../framework/assert.js';
import { TestSuite } from '../framework/types.js';
import { bytes } from './helpers.js';
import { VALID_TRON_ADDRESSES } from './market-helpers.js';

/**
 * 合规专项回归：在限制场景下验证 market 挂单不会成功。
 * 当前先以“失败即通过”的黑盒约束测试实现，适配不同 runtime 上的市场合规限制来源。
 */
export const complianceBlackoutMarketRestrictionSuite: TestSuite = {
  id: 'compliance-blackout-market-restriction',
  title: 'Compliance / blackout market restriction',
  description: 'Attempt a restricted entity market sell order and verify that no new order is created when compliance restrictions are active.',
  tags: ['entity', 'compliance', 'market', 'e2e'],
  async run(ctx) {
    const owner = ctx.actors.ferdie;
    const seller = ctx.actors.bob;
    const tx = ctx.api.tx as any;

    await ctx.step('market actors are funded', async () => {
      await ctx.ensureFundsFor(['ferdie', 'bob'], 25_000);
    });

    const entityId = await ctx.step('create entity token and configure market under restrictive conditions', async () => {
      const createReceipt = await submitTx(
        ctx.api,
        tx.entityRegistry.createEntity(bytes(`compliance-market-${Date.now()}`), null, null, null),
        owner,
        'create compliance market entity',
      );
      assert(createReceipt.success, 'createEntity should succeed');

      const entityIds = await ((ctx.api.query as any).entityRegistry.userEntities?.(owner.address)
        ?? (ctx.api.query as any).entityRegistry.userEntity(owner.address));
      const ids = (entityIds.toJSON() as unknown[]).map((value) => Number(value));
      const entityId = ids[ids.length - 1];
      assert(entityId > 0, 'new market entity id should exist');

      let receipt = await submitTx(
        ctx.api,
        tx.entityToken.createShopToken(entityId, bytes(`cmt-${entityId}`), bytes(`CM${entityId}`), 0, 0, 0),
        owner,
        'create market token',
      );
      assert(receipt.success, 'createShopToken should succeed');

      receipt = await submitTx(ctx.api, tx.entityToken.mintTokens(entityId, seller.address, '1000000'), owner, 'mint seller tokens');
      assert(receipt.success, 'mintTokens seller should succeed');

      receipt = await submitTx(ctx.api, tx.entityMarket.configureMarket(entityId, true, 1, 100), owner, 'configure market');
      assert(receipt.success, 'configureMarket should succeed');

      receipt = await submitTx(ctx.api, tx.entityMarket.setInitialPrice(entityId, 100_000), owner, 'set initial market price');
      assert(receipt.success, 'setInitialPrice should succeed');

      if (tx.entityToken.setTransferRestriction) {
        receipt = await submitTx(ctx.api, tx.entityToken.setTransferRestriction(entityId, 2, 3), owner, 'set restrictive transfer policy');
        assert(receipt.success, 'setTransferRestriction should succeed');
      }

      return entityId;
    });

    await ctx.step('restricted market sell order does not create a new order', async () => {
      const beforeNextOrderId = Number((await (ctx.api.query as any).entityMarket.nextOrderId(entityId)).toString());
      const receipt = await submitTx(
        ctx.api,
        tx.entityMarket.placeSellOrder(entityId, '1000', 120_000, VALID_TRON_ADDRESSES.seller, null),
        seller,
        'attempt restricted entity market sell order',
      );
      assert(!receipt.success, 'restricted market sell order should fail under compliance restriction');

      const afterNextOrderId = Number((await (ctx.api.query as any).entityMarket.nextOrderId(entityId)).toString());
      assert(afterNextOrderId === beforeNextOrderId, 'nextOrderId should not advance when restricted market order is rejected');
    });
  },
};
