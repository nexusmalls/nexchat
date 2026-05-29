import type { ApiPromise } from '@polkadot/api';
import { assert, assertEqual } from './assert.js';
import { normalizeIdentifier } from './codec.js';

export interface CallShape {
  argCount: number;
  argNames: string[];
}

export function assertPallet(api: ApiPromise, group: 'tx' | 'query' | 'events', pallet: string): void {
  const member = (api as any)[group]?.[pallet];
  assert(member != null, `Missing ${group} pallet: ${pallet}`);
}

export function getCallShape(api: ApiPromise, pallet: string, call: string): CallShape {
  const palletTx = (api.tx as any)[pallet];
  assert(palletTx != null, `Missing tx pallet: ${pallet}`);

  const extrinsic = palletTx[call];
  assert(typeof extrinsic === 'function', `Missing extrinsic: ${pallet}.${call}`);

  const argNames = extrinsic.meta.args.map((arg: any) => arg.name.toString());
  return { argCount: argNames.length, argNames };
}

export function assertCallShape(api: ApiPromise, pallet: string, call: string, expectedArgs: string[]): void {
  const actual = getCallShape(api, pallet, call);
  assertEqual(actual.argCount, expectedArgs.length, `Unexpected arg count for ${pallet}.${call}`);

  const normalizedActual = actual.argNames.map(normalizeIdentifier).join(',');
  const normalizedExpected = expectedArgs.map(normalizeIdentifier).join(',');
  assertEqual(normalizedActual, normalizedExpected, `Unexpected arg names for ${pallet}.${call}`);
}

export function assertStorage(api: ApiPromise, pallet: string, storage: string): void {
  const palletQuery = (api.query as any)[pallet];
  assert(palletQuery != null, `Missing query pallet: ${pallet}`);
  assert(palletQuery[storage] != null, `Missing storage accessor: ${pallet}.${storage}`);
}

export function assertStorageAny(api: ApiPromise, pallet: string, storageOptions: string[]): void {
  const palletQuery = (api.query as any)[pallet];
  assert(palletQuery != null, `Missing query pallet: ${pallet}`);

  const matched = storageOptions.find((storage) => palletQuery[storage] != null);
  assert(matched != null, `Missing storage accessor: ${pallet}.${storageOptions.join(' | ')}`);
}

export function assertEvent(api: ApiPromise, pallet: string, event: string): void {
  const palletEvents = (api.events as any)[pallet];
  assert(palletEvents != null, `Missing event pallet: ${pallet}`);
  assert(palletEvents[event] != null, `Missing event: ${pallet}.${event}`);
}
