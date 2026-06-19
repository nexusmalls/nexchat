// EN: Global fund pool pallet accounts (modl prefix derivation).
// CN: 全局资金池 pallet 账户（modl 前缀派生）。

import { stringToU8a, u8aConcat } from "@polkadot/util";
import { encodeAddress } from "@polkadot/util-crypto";
import { NEX_SS58 } from "@/wallet/desktopKeyring";

const MODL_PREFIX = stringToU8a("modl");

export type PoolGroup = "core" | "market" | "infra";

export interface PoolDefinition {
  key: string;
  palletId: string;
  group: PoolGroup;
}

/** EN: Pool display labels (zh, aligned with nexus-com-dapp chainInfo). CN: 资金池展示文案。 */
export const POOL_LABELS: Record<string, { name: string; desc: string }> = {
  treasury: { name: "平台国库", desc: "平台收入和存储补贴" },
  burn: { name: "销毁账户", desc: "代币销毁专用" },
  rewardPool: { name: "节点奖励池", desc: "订阅费回退 + 通胀铸币" },
  marketTreasury: { name: "交易手续费", desc: "NEX/USDT 手续费归集" },
  marketSeed: { name: "流动性引导", desc: "初始流动性资金" },
  marketRewards: { name: "Indexer 奖励", desc: "Indexer 激励池" },
  escrow: { name: "托管账户", desc: "争议/托管资金管理" },
  storage: { name: "存储服务", desc: "IPFS 存储费和押金" },
};

export const POOL_DEFS: PoolDefinition[] = [
  { key: "treasury", palletId: "py/trsry", group: "core" },
  { key: "burn", palletId: "py/burn!", group: "core" },
  { key: "rewardPool", palletId: "py/rwdpl", group: "core" },
  { key: "marketTreasury", palletId: "nxm/trsy", group: "market" },
  { key: "marketSeed", palletId: "nxm/seed", group: "market" },
  { key: "marketRewards", palletId: "nxm/rwds", group: "market" },
  { key: "escrow", palletId: "py/escro", group: "infra" },
  { key: "storage", palletId: "py/storg", group: "infra" },
];

// EN: Substrate formula: b"modl" ++ pallet_id(8) → zero-pad to 32 bytes.
// CN: Substrate 公式：b"modl" ++ pallet_id(8) → 零填充至 32 字节。
export function derivePalletAccount(palletId: string): string {
  const palletBytes = stringToU8a(palletId);
  const raw = u8aConcat(MODL_PREFIX, palletBytes);
  const accountBytes = new Uint8Array(32);
  accountBytes.set(raw.subarray(0, Math.min(raw.length, 32)));
  return encodeAddress(accountBytes, NEX_SS58);
}
