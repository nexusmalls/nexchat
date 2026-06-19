// EN: Staking / nominator types (mirrors nexus-com-dapp use-staking subset).
// CN: 质押 / 提名人类型（对齐 nexus-com-dapp use-staking 子集）。

export type StakingRole = "controller" | "stash" | "new";

export interface StakingUnlockChunk {
  value: bigint;
  era: number;
}

export interface StakingLedgerView {
  stash: string;
  total: bigint;
  active: bigint;
  unlocking: StakingUnlockChunk[];
}

export interface StakingOverview {
  supported: boolean;
  stash: string;
  controller: string;
  role: StakingRole;
  ledger: StakingLedgerView | null;
  nominations: string[] | null;
  activeEraIndex: number;
  minNominatorBond: bigint;
  maxNominations: number;
  validators: string[];
}
