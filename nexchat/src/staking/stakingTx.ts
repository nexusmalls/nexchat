// EN: Staking extrinsics (bond / nominate / unbond) via active signer.
// CN: 质押 extrinsic（绑定 / 提名 / 解押）通过当前签名者提交。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { requireSigner } from "@/chain/signer";
import { canonicalAddress } from "@/wallet/address";

function ensureLive(): void {
  if (config.useMock) {
    throw new Error("Mock 模式无法提交链上交易，请设置 VITE_USE_MOCK=false");
  }
}

type SubmittableTx = {
  signAndSend: (...args: unknown[]) => Promise<unknown>;
};

type WalletApi = Awaited<ReturnType<typeof chainClient.getApiForWallet>> & {
  tx: Record<string, Record<string, (...a: unknown[]) => SubmittableTx>>;
  registry: {
    createType: (type: string, value: unknown) => unknown;
    findMetaError: (m: unknown) => { section: string; name: string };
  };
};

async function signBuiltTxWithApi(api: WalletApi, tx: SubmittableTx, label: string): Promise<string> {
  const backend = requireSigner();
  return new Promise<string>((resolve, reject) => {
    const onResult = (result: {
      dispatchError?: { toString: () => string; isModule?: boolean; asModule?: unknown };
      status: { isInBlock: boolean; asInBlock: { toString: () => string } };
    }) => {
      if (result.dispatchError) {
        let msg = result.dispatchError.toString();
        if (result.dispatchError.isModule) {
          const meta = api.registry.findMetaError(result.dispatchError.asModule);
          msg = `${meta.section}.${meta.name}`;
        }
        reject(new Error(`${label} failed: ${msg}`));
        return;
      }
      if (result.status.isInBlock) resolve(result.status.asInBlock.toString());
    };
    if (backend.kind === "pair") {
      tx.signAndSend(backend.pair, onResult).catch(reject);
    } else {
      tx.signAndSend(backend.address, { signer: backend.signer }, onResult).catch(reject);
    }
  });
}

function buildBondExtrinsic(api: WalletApi, controllerAddress: string, amount: bigint): SubmittableTx {
  const staking = api.tx.staking;
  const bondFn = staking.bond as unknown as {
    (...args: unknown[]): SubmittableTx;
    meta?: { args: unknown[] };
  };
  const argLen = bondFn.meta?.args?.length ?? 3;
  let payee: unknown;
  try {
    payee = api.registry.createType("RewardDestination", "Staked");
  } catch {
    payee = "Staked";
  }
  if (argLen === 2) {
    return bondFn(amount, payee);
  }
  const multi = api.registry.createType("MultiAddress", {
    Id: canonicalAddress(controllerAddress),
  });
  return bondFn(multi, amount, payee);
}

export async function bondStaking(controllerAddress: string, amountPlanck: bigint): Promise<string> {
  ensureLive();
  const api = (await chainClient.getApiForWallet()) as WalletApi;
  const tx = buildBondExtrinsic(api, controllerAddress, amountPlanck);
  return signBuiltTxWithApi(api, tx, "staking.bond");
}

export async function bondExtraStaking(amountPlanck: bigint): Promise<string> {
  ensureLive();
  const api = (await chainClient.getApiForWallet()) as WalletApi;
  return signBuiltTxWithApi(api, api.tx.staking.bondExtra(amountPlanck), "staking.bondExtra");
}

export async function unbondStaking(amountPlanck: bigint): Promise<string> {
  ensureLive();
  const api = (await chainClient.getApiForWallet()) as WalletApi;
  return signBuiltTxWithApi(api, api.tx.staking.unbond(amountPlanck), "staking.unbond");
}

export async function nominateStaking(targets: string[]): Promise<string> {
  ensureLive();
  const api = (await chainClient.getApiForWallet()) as WalletApi;
  const canon = targets.map((t) => canonicalAddress(t));
  return signBuiltTxWithApi(api, api.tx.staking.nominate(canon), "staking.nominate");
}

export async function withdrawUnbondedStaking(): Promise<string> {
  ensureLive();
  const api = (await chainClient.getApiForWallet()) as WalletApi;
  return signBuiltTxWithApi(api, api.tx.staking.withdrawUnbonded(0), "staking.withdrawUnbonded");
}
