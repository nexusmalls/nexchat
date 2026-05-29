import { TestSuite } from '../framework/types.js';
import { complianceBlackoutMarketRestrictionSuite } from './compliance-blackout-market-restriction.js';
import { complianceBlackoutTokenRestrictionSuite } from './compliance-blackout-token-restriction.js';
import { commissionMultiLevelBasicSuite } from './commission-multi-level-basic.js';
import { commissionWithdrawalSplitSuite } from './commission-withdrawal-split.js';
import { entitySmokeBuyerCoreFlowSuite } from './entity-smoke-buyer-core-flow.js';
import { entitySmokeCommissionCoreFlowSuite } from './entity-smoke-commission-core-flow.js';
import { entitySmokeGovernanceCoreFlowSuite } from './entity-smoke-governance-core-flow.js';
import { entitySmokeMarketCoreFlowSuite } from './entity-smoke-market-core-flow.js';
import { entitySmokeSellerCoreFlowSuite } from './entity-smoke-seller-core-flow.js';
import { governanceProposalVoteExecuteSuite } from './governance-proposal-vote-execute.js';

/**
 * 默认执行套件列表；当前保持为空，要求显式选择。
 */
export const DEFAULT_SUITES: TestSuite[] = [];

/**
 * 角色化实体冒烟测试套件集合。
 */
export const ENTITY_SMOKE_SUITES: TestSuite[] = [
  entitySmokeBuyerCoreFlowSuite,
  entitySmokeSellerCoreFlowSuite,
  entitySmokeCommissionCoreFlowSuite,
  entitySmokeGovernanceCoreFlowSuite,
  entitySmokeMarketCoreFlowSuite,
];

/**
 * 历史 Phase 1 套件占位；当前保持为空。
 */
export const PHASE1_SUITES: TestSuite[] = [];

/**
 * 治理专项回归套件集合。
 */
export const GOVERNANCE_REGRESSION_SUITES: TestSuite[] = [
  governanceProposalVoteExecuteSuite,
];

/**
 * 高优先级专项回归套件集合。
 */
export const SPECIAL_REGRESSION_SUITES: TestSuite[] = [
  commissionMultiLevelBasicSuite,
  commissionWithdrawalSplitSuite,
  complianceBlackoutTokenRestrictionSuite,
  complianceBlackoutMarketRestrictionSuite,
];

/**
 * 所有可执行测试套件的扁平集合。
 */
export const ALL_SUITES: TestSuite[] = [
  ...DEFAULT_SUITES,
  ...ENTITY_SMOKE_SUITES,
  ...PHASE1_SUITES,
  ...GOVERNANCE_REGRESSION_SUITES,
  ...SPECIAL_REGRESSION_SUITES,
];

/**
 * 通过套件 id 快速定位套件定义。
 */
export const SUITE_MAP = new Map(ALL_SUITES.map((suite) => [suite.id, suite]));
