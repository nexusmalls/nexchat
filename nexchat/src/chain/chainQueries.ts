// EN: Read-only chain metadata + global fund pools (mirrors nexus-com-dapp /chain).
// CN: 链元数据与全局资金池只读查询（对齐 nexus-com-dapp /chain）。

import { derivePalletAccount, POOL_DEFS, type PoolGroup } from "@/chain/poolDefs";

export interface ChainInfo {
  chainName: string;
  nodeName: string;
  nodeVersion: string;
  specName: string;
  specVersion: number;
  implVersion: number;
  ss58Format: number;
  tokenSymbol: string;
  tokenDecimals: number;
  totalIssuance: string;
  peerCount: number;
  isSyncing: boolean;
  bestBlock: number;
  finalizedBlock: number;
}

export interface PoolInfo {
  key: string;
  palletId: string;
  group: PoolGroup;
  address: string;
  free: bigint;
  reserved: bigint;
}

export interface GlobalPoolsData {
  core: PoolInfo[];
  market: PoolInfo[];
  infra: PoolInfo[];
}

type ChainApi = {
  rpc: {
    system: {
      chain: () => Promise<{ toString: () => string }>;
      name: () => Promise<{ toString: () => string }>;
      version: () => Promise<{ toString: () => string }>;
      health: () => Promise<{
        peers: { toNumber: () => number };
        isSyncing: { isTrue: boolean };
      }>;
    };
    chain: {
      getFinalizedHead: () => Promise<unknown>;
      getHeader: (hash: unknown) => Promise<{ number: { toNumber: () => number } }>;
      getBlock: () => Promise<{ block: { header: { number: { toNumber: () => number } } } }>;
    };
  };
  runtimeVersion: {
    specName: { toString: () => string };
    specVersion: { toNumber: () => number };
    implVersion: { toNumber: () => number };
  };
  registry: {
    getChainProperties: () => {
      ss58Format?: { unwrapOr: (v: null) => { toNumber: () => number } | null };
      tokenSymbol?: { unwrapOr: (v: null) => { toJSON: () => unknown } | null };
      tokenDecimals?: { unwrapOr: (v: null) => { toJSON: () => unknown } | null };
    } | null;
  };
  query: {
    balances?: {
      totalIssuance?: () => Promise<{ toString: () => string } | null>;
    };
    system: {
      account: (who: string) => Promise<{
        data: { free: { toString: () => string }; reserved: { toString: () => string } };
      }>;
    };
  };
};

function readTokenMeta(api: ChainApi): { symbol: string; decimals: number; ss58: number } {
  const props = api.registry.getChainProperties();
  const ss58 = props?.ss58Format?.unwrapOr(null)?.toNumber() ?? 273;
  const symbols = props?.tokenSymbol?.unwrapOr(null);
  const decimals = props?.tokenDecimals?.unwrapOr(null);
  const symbolJson = symbols ? symbols.toJSON() : null;
  const decimalJson = decimals ? decimals.toJSON() : null;
  const symbol = Array.isArray(symbolJson) ? ((symbolJson[0] as string) ?? "NEX") : "NEX";
  const decimal = Array.isArray(decimalJson) ? ((decimalJson[0] as number) ?? 12) : 12;
  return { symbol, decimals: decimal, ss58 };
}

// EN: Fetch chain overview, runtime, and network stats.
// CN: 拉取链概览、运行时与网络统计。
export async function fetchChainInfo(api: ChainApi): Promise<ChainInfo> {
  const [chainName, nodeName, nodeVersion, health, finalizedHead, totalIssuanceRaw, bestBlockRaw] =
    await Promise.all([
      api.rpc.system.chain(),
      api.rpc.system.name(),
      api.rpc.system.version(),
      api.rpc.system.health(),
      api.rpc.chain.getFinalizedHead(),
      api.query.balances?.totalIssuance?.() ?? Promise.resolve(null),
      api.rpc.chain.getBlock(),
    ]);

  const finalizedHeader = await api.rpc.chain.getHeader(finalizedHead);
  const { symbol, decimals, ss58 } = readTokenMeta(api);
  const runtime = api.runtimeVersion;

  return {
    chainName: chainName.toString(),
    nodeName: nodeName.toString(),
    nodeVersion: nodeVersion.toString(),
    specName: runtime.specName.toString(),
    specVersion: runtime.specVersion.toNumber(),
    implVersion: runtime.implVersion.toNumber(),
    ss58Format: ss58,
    tokenSymbol: symbol,
    tokenDecimals: decimals,
    totalIssuance: totalIssuanceRaw ? totalIssuanceRaw.toString() : "0",
    peerCount: health.peers.toNumber(),
    isSyncing: health.isSyncing.isTrue,
    bestBlock: bestBlockRaw.block.header.number.toNumber(),
    finalizedBlock: finalizedHeader.number.toNumber(),
  };
}

// EN: Query balances of global modl-derived pool accounts.
// CN: 查询全局 modl 派生资金池账户余额。
export async function fetchGlobalPools(api: ChainApi): Promise<GlobalPoolsData> {
  const pools = await Promise.all(
    POOL_DEFS.map(async (def) => {
      const address = derivePalletAccount(def.palletId);
      const raw = await api.query.system.account(address);
      return {
        key: def.key,
        palletId: def.palletId,
        group: def.group,
        address,
        free: BigInt(raw.data.free.toString()),
        reserved: BigInt(raw.data.reserved.toString()),
      } satisfies PoolInfo;
    }),
  );

  return {
    core: pools.filter((p) => p.group === "core"),
    market: pools.filter((p) => p.group === "market"),
    infra: pools.filter((p) => p.group === "infra"),
  };
}
