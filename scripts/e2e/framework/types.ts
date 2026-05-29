import type { ApiPromise } from '@polkadot/api';
import type { KeyringPair } from '@polkadot/keyring/types';

export interface DevActors {
  alice: KeyringPair;
  bob: KeyringPair;
  charlie: KeyringPair;
  dave: KeyringPair;
  eve: KeyringPair;
  ferdie: KeyringPair;
  [name: string]: KeyringPair;
}

export interface ChainSnapshot {
  chain: string;
  nodeName: string;
  nodeVersion: string;
  specName: string;
  specVersion: number;
}

export interface SuiteContext {
  api: ApiPromise;
  actors: DevActors;
  chain: ChainSnapshot;
  step<T>(name: string, fn: () => Promise<T> | T): Promise<T>;
  note(message: string): void;
  ensureFunds(minNex?: number): Promise<void>;
  ensureFundsFor(actorNames: string[], minNex?: number): Promise<void>;
  readMarketPrice(): Promise<number>;
}

export interface TestSuite {
  id: string;
  title: string;
  description: string;
  tags: string[];
  run(ctx: SuiteContext): Promise<void>;
}
