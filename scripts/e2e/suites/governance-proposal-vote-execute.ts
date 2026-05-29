import { submitTx } from '../framework/api.js';
import { assert, assertTxSuccess } from '../framework/assert.js';
import { coerceNumber, readObjectField } from '../framework/codec.js';
import { TestSuite } from '../framework/types.js';
import {
  bytes,
  proposalStatusOf,
  readGovernanceConfig,
  readNextProposalId,
  readProposal,
  readProposalTally,
  readVoteRecordMaybe,
  setupFreshEntity,
  waitForProposalStatus,
  waitUntilBlock,
} from './helpers.js';

const INITIAL_QUORUM = 1;
const UPDATED_QUORUM = 2;

/**
 * 实体治理专项回归：创建提案、投票、finalize、execute，并验证治理配置真正更新。
 */
export const governanceProposalVoteExecuteSuite: TestSuite = {
  id: 'governance-proposal-vote-execute',
  title: 'Governance / proposal vote execute',
  description: 'Create a quorum-change proposal, pass it, execute it, and verify governance config is updated on-chain.',
  tags: ['entity', 'governance', 'proposal', 'e2e'],
  async run(ctx) {
    const owner = ctx.actors.eve;
    const voter = ctx.actors.alice;
    const tx = ctx.api.tx as any;

    await ctx.step('governance actors are funded', async () => {
      await ctx.ensureFundsFor(['eve', 'alice'], 25_000);
    });

    const setup = await ctx.step('create a governance-ready entity with token holders and baseline config', async () => {
      const { entityId, shopId } = await setupFreshEntity(ctx.api, owner);

      let receipt = await submitTx(
        ctx.api,
        tx.entityToken.createShopToken(entityId, bytes(`gov-exec-token-${entityId}`), bytes(`GE${entityId}`), 0, 0, 0),
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
          quorum_threshold: INITIAL_QUORUM,
          pass_threshold: 1,
          proposal_threshold: 1,
          admin_veto_enabled: false,
        }),
        owner,
        'set baseline governance config',
      );
      assertTxSuccess(receipt, 'updateGovernanceConfig should succeed');

      const config = await readGovernanceConfig(ctx.api, entityId);
      const quorum = coerceNumber(readObjectField(config, 'quorumThreshold', 'quorum_threshold'));
      assert(quorum === INITIAL_QUORUM, 'baseline governance quorum should be initialized');

      ctx.note(`entityId=${entityId} shopId=${shopId} quorumBefore=${quorum}`);
      return { entityId, shopId };
    });

    const proposalId = await ctx.step('create a quorum-change governance proposal', async () => {
      const nextProposalId = await readNextProposalId(ctx.api);
      const receipt = await submitTx(
        ctx.api,
        tx.entityGovernance.createProposal(
          setup.entityId,
          { QuorumChange: { new_quorum: UPDATED_QUORUM } },
          bytes('governance proposal vote execute'),
          null,
        ),
        owner,
        'create quorum-change proposal',
      );
      assertTxSuccess(receipt, 'createProposal should succeed');

      const proposal = await readProposal(ctx.api, nextProposalId);
      const status = proposalStatusOf(proposal);
      assert(status.includes('Voting'), 'new proposal should enter voting state');
      return nextProposalId;
    });

    await ctx.step('voter casts a yes vote and vote storage is visible', async () => {
      const receipt = await submitTx(
        ctx.api,
        tx.entityGovernance.vote(proposalId, 'Yes'),
        voter,
        'vote yes',
      );
      assertTxSuccess(receipt, 'vote should succeed');

      const proposal = await readProposal(ctx.api, proposalId);
      const tally = readProposalTally(proposal);
      assert(tally.yes > 0, 'proposal yesVotes should increase after vote');

      const voteRecord = await readVoteRecordMaybe(ctx.api, proposalId, voter.address);
      assert(voteRecord != null, 'vote record should exist after vote');
      ctx.note(`proposalId=${proposalId} yesVotes=${tally.yes}`);
    });

    const finalizedProposal = await ctx.step('finalize the proposal after voting period ends', async () => {
      const createdProposal = await readProposal(ctx.api, proposalId);
      const votingEnd = coerceNumber(readObjectField(createdProposal.json, 'votingEnd', 'voting_end'));
      assert(votingEnd != null, 'proposal voting_end should be readable');

      await waitUntilBlock(ctx.api, votingEnd + 1);

      const receipt = await submitTx(
        ctx.api,
        tx.entityGovernance.finalizeVoting(proposalId),
        owner,
        'finalize governance proposal',
      );
      assertTxSuccess(receipt, 'finalizeVoting should succeed');

      const proposal = await waitForProposalStatus(ctx.api, proposalId, (status) => status.includes('Passed'));
      const executionTime = coerceNumber(readObjectField(proposal.json, 'executionTime', 'execution_time'));
      assert(executionTime != null, 'passed proposal should have execution_time');
      return proposal;
    });

    await ctx.step('execute the passed proposal and verify governance config changes on-chain', async () => {
      const executionTime = coerceNumber(readObjectField(finalizedProposal.json, 'executionTime', 'execution_time'));
      assert(executionTime != null, 'execution_time should exist before execute');

      await waitUntilBlock(ctx.api, executionTime);

      const receipt = await submitTx(
        ctx.api,
        tx.entityGovernance.executeProposal(proposalId),
        owner,
        'execute governance proposal',
      );
      assertTxSuccess(receipt, 'executeProposal should succeed');

      const proposal = await waitForProposalStatus(ctx.api, proposalId, (status) => status.includes('Executed'));
      assert(proposalStatusOf(proposal).includes('Executed'), 'proposal should become executed');

      const updatedConfig = await readGovernanceConfig(ctx.api, setup.entityId);
      const updatedQuorum = coerceNumber(readObjectField(updatedConfig, 'quorumThreshold', 'quorum_threshold'));
      assert(updatedQuorum === UPDATED_QUORUM, 'governance quorum should be updated by executed proposal');
      ctx.note(`proposalId=${proposalId} quorumAfter=${updatedQuorum}`);
    });
  },
};
