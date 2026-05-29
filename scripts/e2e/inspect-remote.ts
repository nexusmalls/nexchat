#!/usr/bin/env tsx

const DEFAULT_REMOTE_WS_URL = 'wss://202.140.140.202';

process.env.WS_URL ??= DEFAULT_REMOTE_WS_URL;
process.env.NODE_TLS_REJECT_UNAUTHORIZED ??= '0';

const { connectApi, disconnectApi, captureChainSnapshot } = await import('./framework/api.js');
const { getDevActors, readFreeBalance } = await import('./framework/accounts.js');
const { codecToJson } = await import('./framework/codec.js');

/**
 * 主入口：读取远程链快照、终局推进状态和测试账户余额样本。
 */
async function main(): Promise<void> {
  const api = await connectApi(process.env.WS_URL);
  try {
    const chain = await captureChainSnapshot(api);
    const finalizedHead1 = await api.rpc.chain.getFinalizedHead();
    const finalizedHeader1 = await api.rpc.chain.getHeader(finalizedHead1);
    await new Promise((resolve) => setTimeout(resolve, 6_000));
    const finalizedHead2 = await api.rpc.chain.getFinalizedHead();
    const finalizedHeader2 = await api.rpc.chain.getHeader(finalizedHead2);

    const subscriptionSample: Array<{ number: string; hash: string } | { error: string }> = [];

    try {
      await new Promise<void>(async (resolve) => {
        let unsubscribe: undefined | (() => Promise<void> | void);

        const timeout = setTimeout(async () => {
          if (unsubscribe) {
            await unsubscribe();
          }
          resolve();
        }, 10_000);

        unsubscribe = await api.rpc.chain.subscribeFinalizedHeads(async (header) => {
          subscriptionSample.push({ number: header.number.toString(), hash: header.hash.toHex() });

          if (subscriptionSample.length >= 2) {
            clearTimeout(timeout);
            if (unsubscribe) {
              await unsubscribe();
            }
            resolve();
          }
        });
      });
    } catch (error) {
      subscriptionSample.push({ error: error instanceof Error ? error.message : String(error) });
    }

    const actors = await getDevActors();
    const balances = await Promise.all(
      ['alice', 'bob', 'charlie', 'dave', 'eve', 'ferdie'].map(async (name) => ({
        name,
        address: actors[name].address,
        free: await readFreeBalance(api, actors[name].address),
      })),
    );

    const [properties, health, peerId, roles] = await Promise.all([
      api.rpc.system.properties(),
      api.rpc.system.health(),
      api.rpc.system.localPeerId(),
      api.rpc.system.nodeRoles(),
    ]);

    const [marketPaused, priceProtection, nextOrderId, nextTradeId, tradingFeeBps, depositExchangeRate, nextEntityId, nextShopId] = await Promise.all([
      (api.query as any).nexMarket.marketPausedStore(),
      ((api.query as any).nexMarket.priceProtectionStore ?? (api.query as any).nexMarket.priceProtection)(),
      (api.query as any).nexMarket.nextOrderId(),
      (api.query as any).nexMarket.nextUsdtTradeId(),
      (api.query as any).nexMarket.tradingFeeBps(),
      (api.query as any).nexMarket.depositExchangeRate(),
      (api.query as any).entityRegistry.nextEntityId(),
      (api.query as any).entityShop.nextShopId(),
    ]);

    console.log(JSON.stringify({
      url: process.env.WS_URL,
      chain,
      system: {
        properties: properties.toHuman(),
        health: health.toHuman(),
        localPeerId: peerId.toString(),
        roles: roles.toHuman(),
      },
      finalizedProgress: {
        first: { hash: finalizedHead1.toHex(), number: finalizedHeader1.number.toString() },
        second: { hash: finalizedHead2.toHex(), number: finalizedHeader2.number.toString() },
        advanced: finalizedHeader2.number.toBigInt() > finalizedHeader1.number.toBigInt(),
      },
      finalizedSubscriptionSample: subscriptionSample,
      balances: balances.map((item) => ({
        ...item,
        free: item.free.toString(),
      })),
      nexMarket: {
        marketPaused: codecToJson(marketPaused),
        priceProtection: codecToJson(priceProtection),
        nextOrderId: nextOrderId.toString(),
        nextUsdtTradeId: nextTradeId.toString(),
        tradingFeeBps: tradingFeeBps.toString(),
        depositExchangeRate: codecToJson(depositExchangeRate),
      },
      entityRegistry: {
        nextEntityId: nextEntityId.toString(),
      },
      entityShop: {
        nextShopId: nextShopId.toString(),
      },
    }, null, 2));
  } finally {
    await disconnectApi(api);
  }
}

main().catch((error) => {
  console.error(`远程检查失败 / Remote inspect failed: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
});
