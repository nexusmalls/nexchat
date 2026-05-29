import { ApiPromise, WsProvider } from '@polkadot/api';
import type { SubmittableExtrinsic } from '@polkadot/api/types';
import type { KeyringPair } from '@polkadot/keyring/types';
import type { EventRecord } from '@polkadot/types/interfaces';
import { codecToJson } from './codec.js';
import { ChainSnapshot } from './types.js';
import { customTypes, runtimeDefs } from './runtime-defs.js';

export interface TxEvent {
  section: string;
  method: string;
  data: unknown;
}

export interface TxReceipt {
  label: string;
  success: boolean;
  txHash: string;
  blockHash?: string;
  extrinsicIndex?: number;
  events: TxEvent[];
  error?: string;
}

/**
 * 尝试把链上对象中的数字字段安全转成 number。
 */
function toNumberMaybe(value: any): number | undefined {
  if (value == null) {
    return undefined;
  }
  if (typeof value === 'number') {
    return value;
  }
  if (typeof value.toNumber === 'function') {
    return value.toNumber();
  }
  return undefined;
}

/**
 * 将 dispatch error 解析为更可读的模块错误文本。
 */
function decodeDispatchError(api: ApiPromise, dispatchError: any): string {
  try {
    if (dispatchError?.isModule) {
      const decoded = api.registry.findMetaError(dispatchError.asModule);
      return `${decoded.section}.${decoded.name}: ${decoded.docs.join(' ')}`;
    }
  } catch {
    // fall through
  }

  if (typeof dispatchError?.toString === 'function') {
    return dispatchError.toString();
  }

  return '未知调度错误 / Unknown dispatch error';
}

/**
 * 连接到链节点并返回 ApiPromise 实例。
 */
export async function connectApi(wsUrl: string = process.env.WS_URL ?? 'ws://127.0.0.1:9944'): Promise<ApiPromise> {
  const traceLog = process.env.E2E_LOG_STDERR === '1' ? console.error : console.log;
  if (process.env.E2E_TRACE_CONNECT === '1') {
    traceLog(`[连接 / connect] opening ${wsUrl}`);
  }
  const provider = new WsProvider(wsUrl);
  const api = await ApiPromise.create({ provider, types: customTypes, runtime: runtimeDefs });
  if (process.env.E2E_TRACE_CONNECT === '1') {
    traceLog(`[连接 / connect] ready ${wsUrl} spec=${api.runtimeVersion.specName.toString()} v${api.runtimeVersion.specVersion.toString()}`);
  }
  return api;
}

/**
 * 断开链连接。
 */
export async function disconnectApi(api: ApiPromise): Promise<void> {
  await api.disconnect();
}

/**
 * 采集当前链的基础快照信息。
 */
export async function captureChainSnapshot(api: ApiPromise): Promise<ChainSnapshot> {
  const [chain, nodeName, nodeVersion] = await Promise.all([
    api.rpc.system.chain(),
    api.rpc.system.name(),
    api.rpc.system.version(),
  ]);

  return {
    chain: chain.toString(),
    nodeName: nodeName.toString(),
    nodeVersion: nodeVersion.toString(),
    specName: api.runtimeVersion.specName.toString(),
    specVersion: api.runtimeVersion.specVersion.toNumber(),
  };
}

/**
 * 提交交易并等待最终上链，返回统一格式的交易回执。
 */
export async function submitTx(
  api: ApiPromise,
  tx: SubmittableExtrinsic<'promise'>,
  signer: KeyringPair,
  label: string,
): Promise<TxReceipt> {
  const txHash = tx.hash.toHex();
  const timeoutMs = Number(process.env.E2E_TX_TIMEOUT_MS ?? 90_000);
  const traceTx = process.env.E2E_TRACE_TX === '1';
  const traceLog = process.env.E2E_LOG_STDERR === '1' ? console.error : console.log;

  if (traceTx) {
    traceLog(`[交易 / tx:${label}] submit signer=${signer.address} hash=${txHash}`);
  }

  const inclusion = await new Promise<
    | { blockHash: string; txIndex?: number; events: EventRecord[]; dispatchError?: unknown }
    | { error: string }
  >((resolve) => {
    let settled = false;
    let unsubscribe: undefined | (() => void);
    let latestResult:
      | { blockHash: string; txIndex?: number; events: EventRecord[]; dispatchError?: unknown }
      | undefined;

    const finish = (
      result:
        | { blockHash: string; txIndex?: number; events: EventRecord[]; dispatchError?: unknown }
        | { error: string },
    ) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      if (unsubscribe) {
        try {
          unsubscribe();
        } catch {
          // ignore unsubscribe errors
        }
      }
      resolve(result);
    };

    const timeout = setTimeout(() => {
      finish({ error: `Timed out while waiting for finalized status: ${label}` });
    }, timeoutMs);

    tx.signAndSend(signer, (result: any) => {
      if (traceTx) {
        const status = result.status?.type ?? 'Unknown';
        const txIndex = toNumberMaybe(result.txIndex);
        const eventCount = Array.from(result.events ?? []).length;
        const dispatchError = result.dispatchError
          ? decodeDispatchError(api, result.dispatchError)
          : undefined;
        traceLog(
          `[交易 / tx:${label}] status=${status} txIndex=${txIndex ?? 'n/a'} events=${eventCount}${dispatchError ? ` error=${dispatchError}` : ''}`,
        );
      }

      if (result.status?.isInBlock || result.status?.isFinalized) {
        latestResult = {
          blockHash: result.status.isFinalized
            ? result.status.asFinalized.toHex()
            : result.status.asInBlock.toHex(),
          txIndex: toNumberMaybe(result.txIndex),
          events: Array.from(result.events ?? []) as EventRecord[],
          dispatchError: result.dispatchError,
        };
      }

      if (!result.status?.isFinalized) {
        return;
      }
      finish(latestResult ?? {
        blockHash: result.status.asFinalized.toHex(),
        txIndex: toNumberMaybe(result.txIndex),
        events: Array.from(result.events ?? []) as EventRecord[],
        dispatchError: result.dispatchError,
      });
    }).then((unsub) => {
      unsubscribe = unsub;
      if (settled) {
        try {
          unsubscribe();
        } catch {
          // ignore unsubscribe errors
        }
      }
    }).catch((error: Error) => {
      finish({ error: error.message });
    });
  });

  if ('error' in inclusion) {
    return {
      label,
      success: false,
      txHash,
      events: [],
      error: inclusion.error,
    };
  }

  const { blockHash, events: resultEvents, dispatchError } = inclusion;
  const extrinsicIndex = inclusion.txIndex;
  let records = resultEvents;

  if (records.length === 0 && extrinsicIndex != null) {
    const allEventsCodec = await api.query.system.events.at(blockHash);
    const allEvents = Array.from(allEventsCodec as unknown as Iterable<EventRecord>);
    records = allEvents.filter((record) =>
      record.phase.isApplyExtrinsic && record.phase.asApplyExtrinsic.toNumber() === extrinsicIndex,
    );
  }

  const failed = records.find(
    (record) => record.event.section === 'system' && record.event.method === 'ExtrinsicFailed',
  );

  if (dispatchError || failed) {
    return {
      label,
      success: false,
      txHash,
      blockHash,
      extrinsicIndex,
      events: [],
      error: decodeDispatchError(api, dispatchError ?? failed?.event.data[0]),
    };
  }

  const events: TxEvent[] = records
    .filter((record) => record.event.section !== 'system' && record.event.section !== 'transactionPayment')
    .map((record) => ({
      section: record.event.section as string,
      method: record.event.method as string,
      data: codecToJson(record.event.data),
    }));

  return {
    label,
    success: true,
    txHash,
    blockHash,
    extrinsicIndex,
    events,
  };
}
