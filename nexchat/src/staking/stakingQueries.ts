// EN: Read-only staking queries (`staking` + `session` pallets).
// CN: 质押只读查询（`staking` + `session` pallet）。

import type { StakingLedgerView, StakingOverview, StakingRole } from "@/staking/types";
import { canonicalAddress } from "@/wallet/address";

type StakingApi = {
  query: {
    staking?: Record<string, (...args: unknown[]) => Promise<unknown>>;
    session?: Record<string, (...args: unknown[]) => Promise<unknown>>;
  };
  tx?: {
    staking?: Record<string, unknown>;
  };
  consts?: {
    staking?: {
      minNominatorBond?: { toString: () => string };
      maxNominations?: { toString: () => string };
    };
  };
};

type RawOption = {
  isSome?: boolean;
  unwrap?: () => {
    stash?: { toString: () => string };
    total?: { toString: () => string };
    active?: { toString: () => string };
    unlocking?: unknown[];
    targets?: { toString: () => string }[];
    toString: () => string;
  };
};

function readActiveEraIndex(raw: unknown): number {
  try {
    const r = raw as {
      index?: { toNumber: () => number };
      unwrap?: () => { index: { toNumber: () => number } };
    };
    if (r && typeof r.unwrap === "function") {
      return r.unwrap().index.toNumber();
    }
    if (r?.index && typeof r.index.toNumber === "function") return r.index.toNumber();
  } catch {
    /* fall through */
  }
  return 0;
}

function unsupportedOverview(address: string): StakingOverview {
  return {
    supported: false,
    stash: address,
    controller: address,
    role: "new",
    ledger: null,
    nominations: null,
    activeEraIndex: 0,
    minNominatorBond: 0n,
    maxNominations: 16,
    validators: [],
  };
}

// EN: Load nominator staking overview for connected account.
// CN: 加载当前账户的提名人质押概览。
export async function fetchStakingOverview(
  api: StakingApi,
  addressRaw: string,
): Promise<StakingOverview> {
  const address = canonicalAddress(addressRaw);
  const staking = api.query?.staking;
  const session = api.query?.session;
  const stakingTx = api.tx?.staking;

  if (!staking || !stakingTx) {
    return unsupportedOverview(address);
  }

  let stash = address;
  let controller = address;
  let role: StakingRole = "new";

  const ledgerOnAddr = (await staking.ledger!(address)) as RawOption;
  if (ledgerOnAddr?.isSome) {
    const l = ledgerOnAddr.unwrap!();
    stash = l.stash?.toString() ?? address;
    controller = address;
    role = "controller";
  } else {
    const bonded = (await staking.bonded!(address)) as RawOption;
    if (bonded?.isSome) {
      stash = address;
      controller = bonded.unwrap!().toString();
      role = "stash";
    }
  }

  const ledgerOpt = (await staking.ledger!(controller)) as RawOption;
  let ledger: StakingLedgerView | null = null;
  if (ledgerOpt?.isSome) {
    const l = ledgerOpt.unwrap!();
    const unlocking = ((l.unlocking ?? []) as { value: { toString: () => string }; era: { toString: () => string } }[]).map(
      (u) => ({
        value: BigInt(u.value.toString()),
        era: Number(u.era.toString()),
      }),
    );
    ledger = {
      stash: l.stash?.toString() ?? stash,
      total: BigInt(l.total?.toString() ?? "0"),
      active: BigInt(l.active?.toString() ?? "0"),
      unlocking,
    };
  }

  let nominations: string[] | null = null;
  if (ledger) {
    const nom = (await staking.nominators!(stash)) as RawOption;
    if (nom?.isSome) {
      const t = nom.unwrap!().targets ?? [];
      nominations = t.map((a) => a.toString());
    }
  }

  const activeEraRaw = await staking.activeEra!();
  const activeEraIndex = readActiveEraIndex(activeEraRaw);

  let minNominatorBond = 0n;
  try {
    const c = api.consts?.staking?.minNominatorBond;
    if (c) minNominatorBond = BigInt(c.toString());
  } catch {
    /* optional */
  }

  let maxNominations = 16;
  try {
    const m = api.consts?.staking?.maxNominations;
    if (m) maxNominations = Number(m.toString());
  } catch {
    /* optional */
  }

  let validators: string[] = [];
  try {
    if (session?.validators) {
      const v = await session.validators();
      validators = (v as { toString: () => string }[]).map((a) => canonicalAddress(a.toString()));
    }
  } catch {
    validators = [];
  }

  return {
    supported: true,
    stash,
    controller,
    role,
    ledger,
    nominations,
    activeEraIndex,
    minNominatorBond,
    maxNominations,
    validators,
  };
}

// EN: Unlock chunk is withdrawable when current era >= chunk era.
// CN: 当前 era >= 解锁 era 时可领取。
export function isUnlockChunkWithdrawable(chunkEra: number, currentEraIndex: number): boolean {
  return currentEraIndex >= chunkEra;
}
