import type { ApiPromise } from '@polkadot/api';
import { ensureActorBalance, ensureNamedActorBalance } from './accounts.js';
import { codecToJson, coerceNumber, readObjectField } from './codec.js';
import { DevActors } from './types.js';

export async function ensureFundedActors(api: ApiPromise, actors: DevActors, minNex: number = 25_000): Promise<void> {
  await ensureActorBalance(api, actors, minNex);
}

export async function ensureNamedActorsFunded(
  api: ApiPromise,
  actors: DevActors,
  actorNames: string[],
  minNex: number = 25_000,
): Promise<void> {
  await ensureNamedActorBalance(api, actors, actorNames, minNex);
}

export async function readPreferredMarketPrice(api: ApiPromise): Promise<number> {
  const priceProtectionQuery = (api.query as any).nexMarket?.priceProtection
    ?? (api.query as any).nexMarket?.priceProtectionStore;
  if (priceProtectionQuery) {
    const protection = codecToJson(await priceProtectionQuery());
    const initialPrice = coerceNumber(readObjectField(protection, 'initialPrice', 'initial_price'));
    if (initialPrice && initialPrice > 0) {
      return initialPrice;
    }
  }

  const lastTradePriceQuery = (api.query as any).nexMarket?.lastTradePrice;
  if (lastTradePriceQuery) {
    const lastTrade = coerceNumber(codecToJson(await lastTradePriceQuery()));
    if (lastTrade && lastTrade > 0) {
      return lastTrade;
    }
  }

  return 100_000;
}
