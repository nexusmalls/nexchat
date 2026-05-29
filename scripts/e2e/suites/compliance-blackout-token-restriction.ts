import { submitTx } from '../framework/api.js';
import { assert } from '../framework/assert.js';
import { TestSuite } from '../framework/types.js';
import { bytes, readEntityTokenBalance } from './helpers.js';

/**
 * 合规专项回归：在限制场景下验证 token 转账不会成功，并且余额保持不变。
 * 当前先以“失败即通过”的黑盒约束测试实现，适配不同 runtime 上的限制来源（披露处罚/KYC/transfer restriction）。
 */
export const complianceBlackoutTokenRestrictionSuite: TestSuite = {
  id: 'compliance-blackout-token-restriction',
  title: 'Compliance / blackout token restriction',
  description: 'Attempt a restricted entity token transfer and verify it does not change balances when compliance restrictions are active.',
  tags: ['entity', 'compliance', 'token', 'e2e'],
  async run(ctx) {
    const owner = ctx.actors.ferdie;
    const sender = ctx.actors.bob;
    const receiver = ctx.actors.charlie;
    const tx = ctx.api.tx as any;

    await ctx.step('actors are funded', async () => {
      await ctx.ensureFundsFor(['ferdie', 'bob', 'charlie'], 25_000);
    });

    const entityId = await ctx.step('create entity token state with a restrictive transfer configuration', async () => {
      const createReceipt = await submitTx(
        ctx.api,
        tx.entityRegistry.createEntity(bytes(`compliance-token-${Date.now()}`), null, null, null),
        owner,
        'create compliance token entity',
      );
      assert(createReceipt.success, 'createEntity should succeed');

      const entityIds = await ((ctx.api.query as any).entityRegistry.userEntities?.(owner.address)
        ?? (ctx.api.query as any).entityRegistry.userEntity(owner.address));
      const ids = (entityIds.toJSON() as unknown[]).map((value) => Number(value));
      const entityId = ids[ids.length - 1];
      assert(entityId > 0, 'new entity id should exist');

      let receipt = await submitTx(
        ctx.api,
        tx.entityToken.createShopToken(entityId, bytes(`ct-${entityId}`), bytes(`CT${entityId}`), 0, 0, 0),
        owner,
        'create compliance token',
      );
      assert(receipt.success, 'createShopToken should succeed');

      receipt = await submitTx(ctx.api, tx.entityToken.mintTokens(entityId, sender.address, '100000'), owner, 'mint sender token balance');
      assert(receipt.success, 'mintTokens should succeed');

      if (tx.entityToken.setTransferRestriction) {
        receipt = await submitTx(ctx.api, tx.entityToken.setTransferRestriction(entityId, 2, 3), owner, 'set restrictive transfer policy');
        assert(receipt.success, 'setTransferRestriction should succeed');
      }

      return entityId;
    });

    await ctx.step('restricted token transfer does not move balances', async () => {
      const beforeSender = await readEntityTokenBalance(ctx.api, entityId, sender.address);
      const beforeReceiver = await readEntityTokenBalance(ctx.api, entityId, receiver.address);

      const transferCall = tx.entityToken.transferTokens
        ? tx.entityToken.transferTokens(entityId, receiver.address, '1000')
        : tx.entityToken.transfer(entityId, receiver.address, '1000');
      const receipt = await submitTx(ctx.api, transferCall, sender, 'attempt restricted token transfer');
      assert(!receipt.success, 'restricted token transfer should fail under compliance restriction');

      const afterSender = await readEntityTokenBalance(ctx.api, entityId, sender.address);
      const afterReceiver = await readEntityTokenBalance(ctx.api, entityId, receiver.address);

      assert(afterSender === beforeSender, 'sender token balance should remain unchanged after rejected transfer');
      assert(afterReceiver === beforeReceiver, 'receiver token balance should remain unchanged after rejected transfer');
    });
  },
};
