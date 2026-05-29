import { submitTx } from '../framework/api.js';
import { assert, assertTxSuccess } from '../framework/assert.js';
import { coerceNumber, readObjectField } from '../framework/codec.js';
import { TestSuite } from '../framework/types.js';
import {
  bytes,
  readNextProposalId,
  readProposal,
  readVoteRecordMaybe,
  setupFreshEntity,
} from './helpers.js';

/**
 * 提取治理提案状态字符串，兼容 human/json 两种结构。
 */
function proposalStatusOf(proposal: { json: Record<string, unknown>; human: Record<string, unknown> }): string {
  return String(readObjectField(proposal.human, 'status') ?? readObjectField(proposal.json, 'status') ?? '');
}

/**
 * 实体治理核心冒烟流：创建提案、投票并验证提案与投票存储。
 */
export const entitySmokeGovernanceCoreFlowSuite: TestSuite = {
  id: 'entity-smoke-governance-core-flow',
  title: 'Entity smoke / governance core flow',
  description: 'Create an entity governance proposal, cast a vote, and verify proposal plus vote storage are written.',
  tags: ['entity', 'governance', 'smoke'],
  async run(ctx) {
    const owner = ctx.actors.eve;
    const voter = ctx.actors.alice;
    const tx = ctx.api.tx as any;

    await ctx.step('governance actors are funded', async () => {
      await ctx.ensureFundsFor(['eve', 'alice'], 25_000);
    });

    const setup = await ctx.step('create a fresh entity and ensure governance token exists', async () => {
      const { entityId, shopId } = await setupFreshEntity(ctx.api, owner);

      let receipt = await submitTx(
        ctx.api,
        tx.entityToken.createShopToken(entityId, bytes(`gov-token-${entityId}`), bytes(`GOV${entityId}`), 0, 0, 0),
        owner,
        'create governance token base asset',
      );
      assertTxSuccess(receipt, 'createShopToken should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.entityToken.changeTokenType(entityId, 'Governance'),
        owner,
        'change token type to governance',
      );
      assertTxSuccess(receipt, 'changeTokenType should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.entityToken.mintTokens(entityId, owner.address, '500000'),
        owner,
        'mint owner governance tokens',
      );
      assertTxSuccess(receipt, 'mintTokens owner should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.entityToken.mintTokens(entityId, voter.address, '250000'),
        owner,
        'mint voter governance tokens',
      );
      assertTxSuccess(receipt, 'mintTokens voter should succeed');

      receipt = await submitTx(
        ctx.api,
        tx.entityGovernance.updateGovernanceConfig(shopId, {
          mode: 'FullDAO',
          voting_period: 10,
          execution_delay: 1,
          quorum_threshold: 1,
          pass_threshold: 1,
          proposal_threshold: 1,
          admin_veto_enabled: false,
        }),
        owner,
        'set governance config',
      );
      assertTxSuccess(receipt, 'updateGovernanceConfig should succeed');
      return { entityId, shopId };
    });

    const proposalId = await ctx.step('create a governance proposal', async () => {
      const nextProposalId = await readNextProposalId(ctx.api);
      const receipt = await submitTx(
        ctx.api,
        tx.entityGovernance.createProposal(
          setup.shopId,
          { General: { title_cid: bytes('title-cid-smoke'), content_cid: bytes('content-cid-smoke') } },
          bytes('smoke proposal'),
          null,
        ),
        owner,
        'create governance proposal',
      );
      assertTxSuccess(receipt, 'createProposal should succeed');
      return nextProposalId;
    });

    await ctx.step('second actor votes yes and vote storage is visible', async () => {
      const receipt = await submitTx(
        ctx.api,
        tx.entityGovernance.vote(proposalId, 'Yes'),
        voter,
        'vote yes',
      );
      assertTxSuccess(receipt, 'vote should succeed');

      const proposal = await readProposal(ctx.api, proposalId);
      const status = proposalStatusOf(proposal);
      assert(status.includes('Voting') || status.includes('Passed'), 'proposal should stay in voting or become passed');

      const yesVotes = coerceNumber(readObjectField(proposal.json, 'yesVotes', 'yes_votes')) ?? 0;
      assert(yesVotes > 0, 'proposal yesVotes should increase after vote');

      const voteRecord = await readVoteRecordMaybe(ctx.api, proposalId, voter.address);
      assert(voteRecord != null, 'vote record should exist after vote');
    });
  },
};
