#!/usr/bin/env node

import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ApiPromise, Keyring, WsProvider } from '@polkadot/api';
import { cryptoWaitReady } from '@polkadot/util-crypto';

const WS_URL = process.env.WS_URL ?? 'wss://202.140.140.202';
const SS58_FORMAT = 273;
const NEX = 1_000_000_000_000n;
const SELLER_TRON_ADDRESS = 'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t';
const BUYER_TRON_ADDRESS = 'TQn9Y2khEsLJW1ChVWFMSMeRDow5KcbLSE';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ARTIFACT_DIR = path.join(__dirname, 'artifacts');
const RESULTS_PATH = path.join(ARTIFACT_DIR, 'results.json');
const ENVIRONMENT_PATH = path.join(ARTIFACT_DIR, 'environment.json');

const report = {
  startedAt: new Date().toISOString(),
  finishedAt: null,
  wsUrl: WS_URL,
  skippedExistingCoverage: [
    {
      module: 'pallet-commission-single-line',
      reason: '已有现成业务流覆盖，按要求跳过重复测试',
      existingTest: 'e2e/suites/entity-commerce-commission-flow.ts',
      coveredFlow: '单轨配置 + 单轨索引分配',
    },
    {
      module: 'pallet-commission-pool-reward',
      reason: '已有现成业务流覆盖，按要求跳过重复测试',
      existingTest: 'e2e/suites/entity-commerce-commission-flow.ts',
      coveredFlow: '奖池配置 + 累积 + claim',
    },
    {
      module: 'pallet-commission-multi-level',
      reason: '已有现成业务流覆盖，按要求跳过重复测试',
      existingTest: 'e2e/suites/entity-commerce-commission-flow.ts',
      coveredFlow: '多级分佣配置 + 分佣累计',
    },
    {
      module: 'pallet-entity-member',
      reason: '已有现成业务流覆盖，按要求跳过重复测试',
      existingTest: 'e2e/suites/entity-commerce-commission-flow.ts',
      coveredFlow: '会员策略 + 注册 + 激活',
    },
    {
      module: 'pallet-entity-loyalty',
      reason: '仅跳过已覆盖的购物金路径；本次补测积分路径',
      existingTest: 'e2e/suites/entity-commerce-commission-flow.ts',
      coveredFlow: '购物金入账 + 消费',
    },
    {
      module: 'pallet-entity-product',
      reason: '仅跳过已覆盖的数字商品路径；本次补测实物商品路径',
      existingTest: 'e2e/suites/entity-commerce-commission-flow.ts',
      coveredFlow: '数字商品创建 + 发布',
    },
    {
      module: 'pallet-entity-order',
      runtimePallet: 'entityTransaction',
      reason: '仅跳过已覆盖的数字订单即时完成路径；本次补测实物发货/收货路径',
      existingTest: 'e2e/suites/entity-commerce-commission-flow.ts',
      coveredFlow: '数字订单下单 + 自动完成',
    },
    {
      module: 'pallet-nex-market',
      reason: '仅跳过已覆盖的挂单/撤单路径；本次补测撮合/付款/确认收款路径',
      existingTest: 'e2e/suites/nex-market-smoke.ts',
      coveredFlow: 'placeSellOrder/placeBuyOrder + cancelOrder',
    },
  ],
  environment: {},
  suites: [],
  summary: {},
};

function humanJson(value) {
  if (value && typeof value.toHuman === 'function') {
    return value.toHuman();
  }
  if (value && typeof value.toJSON === 'function') {
    return value.toJSON();
  }
  return value;
}

function codecJson(value) {
  if (value && typeof value.toJSON === 'function') {
    return value.toJSON();
  }
  return value;
}

function normalizeIdentifier(value) {
  return String(value).replace(/[^a-z0-9]/gi, '').toLowerCase();
}

function readField(record, ...candidates) {
  if (!record || typeof record !== 'object') {
    return undefined;
  }

  for (const candidate of candidates) {
    if (candidate in record) {
      return record[candidate];
    }

    const normalizedCandidate = normalizeIdentifier(candidate);
    for (const key of Object.keys(record)) {
      if (normalizeIdentifier(key) === normalizedCandidate) {
        return record[key];
      }
    }
  }

  return undefined;
}

function asBigInt(value) {
  if (typeof value === 'bigint') {
    return value;
  }
  if (typeof value === 'number') {
    return BigInt(value);
  }
  if (typeof value === 'string') {
    return BigInt(value.replace(/,/g, '').trim());
  }
  if (value && typeof value.toString === 'function') {
    return BigInt(value.toString());
  }
  throw new Error(`Unable to convert value to bigint: ${String(value)}`);
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function expectEvent(txResult, eventName, message) {
  assert(
    txResult.events.includes(eventName),
    `${message}: missing ${eventName}, actual=[${txResult.events.join(', ')}]`,
  );
}

function decodeDispatchError(api, dispatchError) {
  if (!dispatchError) {
    return null;
  }

  if (dispatchError.isModule) {
    const meta = api.registry.findMetaError(dispatchError.asModule);
    return `${meta.section}.${meta.name}: ${meta.docs.join(' ')}`;
  }

  return dispatchError.toString();
}

function toEventName(record) {
  return `${record.event.section}.${record.event.method}`;
}

function nex(amount) {
  return BigInt(amount) * NEX;
}

async function writeJson(targetPath, value) {
  await fs.mkdir(path.dirname(targetPath), { recursive: true });
  await fs.writeFile(targetPath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

async function submitTx(api, tx, signer, label) {
  return await new Promise((resolve, reject) => {
    let unsubscribe;

    tx.signAndSend(signer, (result) => {
      if (!result.status?.isFinalized) {
        return;
      }

      try {
        unsubscribe?.();
      } catch {
        // ignore unsubscribe errors
      }

      const error = decodeDispatchError(api, result.dispatchError);
      const events = result.events.map(toEventName);
      const failed = events.includes('system.ExtrinsicFailed');

      resolve({
        label,
        ok: !error && !failed,
        error: error ?? (failed ? 'system.ExtrinsicFailed' : null),
        txHash: tx.hash.toHex(),
        blockHash: result.status.asFinalized.toHex(),
        events,
      });
    }).then((unsub) => {
      unsubscribe = unsub;
    }).catch(reject);
  });
}

async function readFreeBalance(api, address) {
  const account = await api.query.system.account(address);
  return BigInt(account.data.free.toString());
}

async function ensureBalance(api, funder, actor, minimum) {
  const free = await readFreeBalance(api, actor.address);
  if (free >= minimum) {
    return {
      funded: false,
      before: free.toString(),
      after: free.toString(),
    };
  }

  const delta = minimum - free;
  const txResult = await submitTx(
    api,
    api.tx.balances.transferKeepAlive(actor.address, delta.toString()),
    funder,
    `fund ${actor.meta.name ?? actor.address}`,
  );

  assert(txResult.ok, `Funding ${actor.address} failed: ${txResult.error ?? 'unknown error'}`);
  expectEvent(txResult, 'balances.Transfer', 'Funding should emit balances.Transfer');

  const after = await readFreeBalance(api, actor.address);
  return {
    funded: true,
    before: free.toString(),
    after: after.toString(),
  };
}

async function readEntityIds(api, address) {
  const query = api.query.entityRegistry.userEntities ?? api.query.entityRegistry.userEntity;
  assert(typeof query === 'function', 'Missing entityRegistry user entity index query');
  const value = await query(address);
  const json = codecJson(value);
  return Array.isArray(json) ? json.map((item) => Number(item)) : [];
}

async function readEntity(api, entityId) {
  const value = await api.query.entityRegistry.entities(entityId);
  assert(value.isSome, `entity ${entityId} should exist`);
  const entity = value.unwrap();
  return {
    json: codecJson(entity),
    human: humanJson(entity),
  };
}

async function readShop(api, shopId) {
  const value = await api.query.entityShop.shops(shopId);
  assert(value.isSome, `shop ${shopId} should exist`);
  const shop = value.unwrap();
  return {
    json: codecJson(shop),
    human: humanJson(shop),
  };
}

async function readProduct(api, productId) {
  const value = await api.query.entityProduct.products(productId);
  assert(value.isSome, `product ${productId} should exist`);
  const product = value.unwrap();
  return {
    json: codecJson(product),
    human: humanJson(product),
  };
}

async function readEntityOrder(api, orderId) {
  const value = await api.query.entityTransaction.orders(orderId);
  assert(value.isSome, `entity order ${orderId} should exist`);
  const order = value.unwrap();
  return {
    json: codecJson(order),
    human: humanJson(order),
  };
}

async function readMarketOrder(api, orderId) {
  const value = await api.query.nexMarket.orders(orderId);
  assert(value.isSome, `nex market order ${orderId} should exist`);
  const order = value.unwrap();
  return {
    json: codecJson(order),
    human: humanJson(order),
  };
}

async function readMarketTrade(api, tradeId) {
  const value = await api.query.nexMarket.usdtTrades(tradeId);
  assert(value.isSome, `nex market trade ${tradeId} should exist`);
  const trade = value.unwrap();
  return {
    json: codecJson(trade),
    human: humanJson(trade),
  };
}

async function captureEnvironment(api, actors) {
  const [chain, nodeName, nodeVersion, finalizedHead] = await Promise.all([
    api.rpc.system.chain(),
    api.rpc.system.name(),
    api.rpc.system.version(),
    api.rpc.chain.getFinalizedHead(),
  ]);
  const finalizedHeader = await api.rpc.chain.getHeader(finalizedHead);

  const balances = {};
  for (const [name, actor] of Object.entries(actors)) {
    balances[name] = {
      address: actor.address,
      free: (await readFreeBalance(api, actor.address)).toString(),
      entityIds: await readEntityIds(api, actor.address),
      marketOrderIds: codecJson(await api.query.nexMarket.userOrders(actor.address)),
    };
  }

  return {
    chain: chain.toString(),
    nodeName: nodeName.toString(),
    nodeVersion: nodeVersion.toString(),
    specName: api.runtimeVersion.specName.toString(),
    specVersion: api.runtimeVersion.specVersion.toNumber(),
    finalizedBlock: finalizedHeader.number.toString(),
    nextEntityId: (await api.query.entityRegistry.nextEntityId()).toString(),
    nextShopId: (await api.query.entityShop.nextShopId()).toString(),
    nextProductId: (await api.query.entityProduct.nextProductId()).toString(),
    nextEntityOrderId: (await api.query.entityTransaction.nextOrderId()).toString(),
    nextMarketOrderId: (await api.query.nexMarket.nextOrderId()).toString(),
    nextMarketTradeId: (await api.query.nexMarket.nextUsdtTradeId()).toString(),
    balances,
  };
}

function startSuite(id, title, description) {
  console.log(`\n[SUITE] ${title} (${id})`);
  console.log(`        ${description}`);
  const suite = {
    id,
    title,
    description,
    startedAt: new Date().toISOString(),
    finishedAt: null,
    status: 'running',
    steps: [],
    summary: {},
  };
  report.suites.push(suite);
  return suite;
}

async function runStep(suite, name, fn) {
  console.log(`[STEP] ${name}`);
  const step = {
    name,
    startedAt: new Date().toISOString(),
    finishedAt: null,
    durationMs: 0,
    status: 'running',
    notes: [],
  };
  suite.steps.push(step);

  const started = Date.now();
  const note = (message) => {
    step.notes.push(message);
  };

  try {
    const result = await fn(note);
    step.status = 'passed';
    step.result = result;
    console.log(`[PASS] ${name}`);
    return result;
  } catch (error) {
    step.status = 'failed';
    step.error = error instanceof Error ? error.message : String(error);
    suite.status = 'failed';
    console.log(`[FAIL] ${name}: ${step.error}`);
    throw error;
  } finally {
    step.finishedAt = new Date().toISOString();
    step.durationMs = Date.now() - started;
  }
}

async function runEntityExtensionSuite(api, actors) {
  const suite = startSuite(
    'entity-extension-flow',
    'Entity shop + loyalty points + physical order flow',
    '补测 entity-shop / entity-loyalty(积分) / entity-product(实物) / entity-order(实物发货收货) 的缺失业务流。',
  );

  try {
    const owner = await runStep(suite, 'select a low-usage entity owner and fund participants', async (note) => {
      const candidates = [actors.charlie, actors.dave, actors.bob, actors.ferdie, actors.alice];
      const candidateRows = [];

      for (const actor of candidates) {
        const entityIds = await readEntityIds(api, actor.address);
        candidateRows.push({
          name: actor.meta.name ?? actor.address,
          address: actor.address,
          entityIds,
        });
      }

      candidateRows.sort((left, right) => left.entityIds.length - right.entityIds.length);
      const picked = candidateRows.find((row) => row.entityIds.length < 3);
      assert(picked, 'No actor with available entity quota was found');

      const selectedActor = Object.values(actors).find((actor) => actor.address === picked.address);
      assert(selectedActor, 'Selected owner actor was not found in actor map');

      note(`owner=${picked.name} existingEntityIds=${JSON.stringify(picked.entityIds)}`);

      const ownerFunding = await ensureBalance(api, actors.alice, selectedActor, nex(500));
      const buyerFunding = await ensureBalance(api, actors.alice, actors.bob, nex(100));
      const pointsRecipientFunding = await ensureBalance(api, actors.alice, actors.dave, nex(100));

      note(`ownerFunding=${JSON.stringify(ownerFunding)}`);
      note(`buyerFunding=${JSON.stringify(buyerFunding)}`);
      note(`pointsRecipientFunding=${JSON.stringify(pointsRecipientFunding)}`);

      return {
        ownerName: selectedActor.meta.name ?? selectedActor.address,
        ownerAddress: selectedActor.address,
      };
    });

    const entityContext = await runStep(suite, 'create entity, create secondary shop, switch primary, and withdraw operating fund', async (note) => {
      const ownerActor = Object.values(actors).find((actor) => actor.address === owner.ownerAddress);
      assert(ownerActor, 'Owner actor could not be resolved');

      const nextEntityId = Number((await api.query.entityRegistry.nextEntityId()).toString());
      const createEntityResult = await submitTx(
        api,
        api.tx.entityRegistry.createEntity(`biz-${Date.now()}`, null, null, null),
        ownerActor,
        'create entity',
      );
      assert(createEntityResult.ok, `create entity failed: ${createEntityResult.error ?? 'unknown error'}`);
      expectEvent(createEntityResult, 'entityRegistry.EntityCreated', 'create entity should emit EntityCreated');

      const entity = await readEntity(api, nextEntityId);
      const autoPrimaryShopId = Number(readField(entity.json, 'primaryShopId', 'primary_shop_id'));
      assert(autoPrimaryShopId > 0, 'Entity should have an auto-created primary shop');
      note(`entityId=${nextEntityId} autoPrimaryShopId=${autoPrimaryShopId}`);

      const createShopResult = await submitTx(
        api,
        api.tx.entityShop.createShop(
          nextEntityId,
          `branch-${Date.now()}`,
          api.createType('PalletEntityCommonShopType', 'OnlineStore'),
          nex(200).toString(),
        ),
        ownerActor,
        'create secondary shop',
      );
      assert(createShopResult.ok, `create secondary shop failed: ${createShopResult.error ?? 'unknown error'}`);
      expectEvent(createShopResult, 'entityShop.ShopCreated', 'create secondary shop should emit ShopCreated');

      const secondaryShopId = Number((await api.query.entityShop.nextShopId()).toString()) - 1;
      const secondaryShop = await readShop(api, secondaryShopId);
      note(`secondaryShopId=${secondaryShopId}`);

      const setPrimaryResult = await submitTx(
        api,
        api.tx.entityShop.setPrimaryShop(nextEntityId, secondaryShopId),
        ownerActor,
        'set primary shop',
      );
      assert(setPrimaryResult.ok, `set primary shop failed: ${setPrimaryResult.error ?? 'unknown error'}`);
      expectEvent(setPrimaryResult, 'entityShop.PrimaryShopChanged', 'set primary shop should emit PrimaryShopChanged');

      const entityAfterPrimarySwitch = await readEntity(api, nextEntityId);
      const currentPrimaryShopId = Number(readField(entityAfterPrimarySwitch.json, 'primaryShopId', 'primary_shop_id'));
      assert(currentPrimaryShopId === secondaryShopId, 'Primary shop id should switch to the secondary shop');

      const withdrawResult = await submitTx(
        api,
        api.tx.entityShop.withdrawOperatingFund(secondaryShopId, nex(50).toString()),
        ownerActor,
        'withdraw operating fund',
      );
      assert(withdrawResult.ok, `withdraw operating fund failed: ${withdrawResult.error ?? 'unknown error'}`);
      expectEvent(withdrawResult, 'entityShop.OperatingFundWithdrawn', 'withdraw should emit OperatingFundWithdrawn');

      return {
        entityId: nextEntityId,
        autoPrimaryShopId,
        secondaryShopId,
        secondaryShop: secondaryShop.human,
      };
    });

    await runStep(suite, 'enable loyalty points, issue, transfer, and redeem', async (note) => {
      const ownerActor = Object.values(actors).find((actor) => actor.address === owner.ownerAddress);
      assert(ownerActor, 'Owner actor could not be resolved');

      const enableResult = await submitTx(
        api,
        api.tx.entityLoyalty.enablePoints(
          entityContext.secondaryShopId,
          'Flow Points',
          'FP',
          100,
          100,
          true,
        ),
        ownerActor,
        'enable points',
      );
      assert(enableResult.ok, `enable points failed: ${enableResult.error ?? 'unknown error'}`);
      expectEvent(enableResult, 'entityLoyalty.ShopPointsEnabled', 'enable points should emit ShopPointsEnabled');

      const issueResult = await submitTx(
        api,
        api.tx.entityLoyalty.managerIssuePoints(entityContext.secondaryShopId, actors.bob.address, '1000'),
        ownerActor,
        'issue points',
      );
      assert(issueResult.ok, `issue points failed: ${issueResult.error ?? 'unknown error'}`);
      expectEvent(issueResult, 'entityLoyalty.PointsIssued', 'issue points should emit PointsIssued');

      const transferResult = await submitTx(
        api,
        api.tx.entityLoyalty.transferPoints(entityContext.secondaryShopId, actors.dave.address, '300'),
        actors.bob,
        'transfer points',
      );
      assert(transferResult.ok, `transfer points failed: ${transferResult.error ?? 'unknown error'}`);
      expectEvent(transferResult, 'entityLoyalty.PointsTransferred', 'transfer points should emit PointsTransferred');

      const redeemResult = await submitTx(
        api,
        api.tx.entityLoyalty.redeemPoints(entityContext.secondaryShopId, '100'),
        actors.dave,
        'redeem points',
      );
      assert(redeemResult.ok, `redeem points failed: ${redeemResult.error ?? 'unknown error'}`);
      expectEvent(redeemResult, 'entityLoyalty.PointsRedeemed', 'redeem points should emit PointsRedeemed');

      const bobPoints = asBigInt(await api.query.entityLoyalty.shopPointsBalances(entityContext.secondaryShopId, actors.bob.address));
      const davePoints = asBigInt(await api.query.entityLoyalty.shopPointsBalances(entityContext.secondaryShopId, actors.dave.address));
      assert(bobPoints === 700n, `Bob points balance should be 700, got ${bobPoints}`);
      assert(davePoints === 200n, `Dave points balance should be 200 after redeem, got ${davePoints}`);

      note(`bobPoints=${bobPoints.toString()} davePoints=${davePoints.toString()}`);
      return {
        bobPoints: bobPoints.toString(),
        davePoints: davePoints.toString(),
      };
    });

    const productContext = await runStep(suite, 'create and publish a physical product on the secondary shop', async (note) => {
      const ownerActor = Object.values(actors).find((actor) => actor.address === owner.ownerAddress);
      assert(ownerActor, 'Owner actor could not be resolved');

      const nextProductId = Number((await api.query.entityProduct.nextProductId()).toString());
      const createProductResult = await submitTx(
        api,
        api.tx.entityProduct.createProduct(
          entityContext.secondaryShopId,
          `name-${Date.now()}`,
          `img-${Date.now()}`,
          `detail-${Date.now()}`,
          nex(10).toString(),
          0,
          10,
          api.createType('PalletEntityCommonProductCategory', 'Physical'),
          0,
          '',
          '',
          1,
          5,
          api.createType('PalletEntityCommonProductVisibility', 'Public'),
        ),
        ownerActor,
        'create physical product',
      );
      assert(createProductResult.ok, `create physical product failed: ${createProductResult.error ?? 'unknown error'}`);
      expectEvent(createProductResult, 'entityProduct.ProductCreated', 'create physical product should emit ProductCreated');

      const publishResult = await submitTx(
        api,
        api.tx.entityProduct.publishProduct(nextProductId),
        ownerActor,
        'publish physical product',
      );
      assert(publishResult.ok, `publish physical product failed: ${publishResult.error ?? 'unknown error'}`);
      expectEvent(publishResult, 'entityProduct.ProductStatusChanged', 'publish should emit ProductStatusChanged');

      const product = await readProduct(api, nextProductId);
      const status = String(readField(product.human, 'status'));
      const category = String(readField(product.human, 'category'));
      assert(status.toLowerCase().includes('onsale'), `Product status should be OnSale, got ${status}`);
      assert(category.toLowerCase().includes('physical'), `Product category should be Physical, got ${category}`);

      note(`productId=${nextProductId} status=${status}`);
      return {
        productId: nextProductId,
      };
    });

    await runStep(suite, 'place a physical order, ship it, and confirm receipt', async (note) => {
      const ownerActor = Object.values(actors).find((actor) => actor.address === owner.ownerAddress);
      assert(ownerActor, 'Owner actor could not be resolved');

      const nextOrderId = Number((await api.query.entityTransaction.nextOrderId()).toString());
      const placeOrderResult = await submitTx(
        api,
        api.tx.entityTransaction.placeOrder(
          productContext.productId,
          1,
          `ship-${Date.now()}`,
          null,
          null,
          null,
          `note-${Date.now()}`,
          null,
        ),
        actors.bob,
        'place physical order',
      );
      assert(placeOrderResult.ok, `place physical order failed: ${placeOrderResult.error ?? 'unknown error'}`);
      expectEvent(placeOrderResult, 'entityTransaction.OrderCreated', 'place physical order should emit OrderCreated');

      let order = await readEntityOrder(api, nextOrderId);
      let status = String(readField(order.human, 'status'));
      assert(status.toLowerCase().includes('paid'), `Physical order should be Paid after placeOrder, got ${status}`);

      const shipResult = await submitTx(
        api,
        api.tx.entityTransaction.shipOrder(nextOrderId, `track-${Date.now()}`),
        ownerActor,
        'ship physical order',
      );
      assert(shipResult.ok, `ship physical order failed: ${shipResult.error ?? 'unknown error'}`);
      expectEvent(shipResult, 'entityTransaction.OrderShipped', 'ship order should emit OrderShipped');

      order = await readEntityOrder(api, nextOrderId);
      status = String(readField(order.human, 'status'));
      assert(status.toLowerCase().includes('shipped'), `Physical order should be Shipped after shipOrder, got ${status}`);

      const confirmResult = await submitTx(
        api,
        api.tx.entityTransaction.confirmReceipt(nextOrderId),
        actors.bob,
        'confirm physical order receipt',
      );
      assert(confirmResult.ok, `confirm receipt failed: ${confirmResult.error ?? 'unknown error'}`);
      expectEvent(confirmResult, 'entityTransaction.OrderCompleted', 'confirm receipt should emit OrderCompleted');

      order = await readEntityOrder(api, nextOrderId);
      status = String(readField(order.human, 'status'));
      assert(status.toLowerCase().includes('completed'), `Physical order should be Completed after confirmReceipt, got ${status}`);

      note(`entityOrderId=${nextOrderId} finalStatus=${status}`);
      return {
        entityOrderId: nextOrderId,
        finalStatus: status,
      };
    });

    suite.status = suite.status === 'failed' ? 'failed' : 'passed';
  } catch (error) {
    suite.status = 'failed';
    suite.summary.error = error instanceof Error ? error.message : String(error);
    throw error;
  } finally {
    suite.finishedAt = new Date().toISOString();
  }
}

async function runNexMarketTradeSuite(api, actors) {
  const suite = startSuite(
    'nex-market-trade-flow',
    'NEX market reserve/accept + payment + seller confirmation flow',
    '补测 nex-market 未被现有 smoke 覆盖的撮合成交链路。',
  );

  try {
    const marketContext = await runStep(suite, 'prepare market seller and read baseline market state', async (note) => {
      const sellerCandidates = [actors.charlie, actors.dave, actors.ferdie, actors.bob];
      const candidateRows = [];

      for (const actor of sellerCandidates) {
        const orderIds = codecJson(await api.query.nexMarket.userOrders(actor.address));
        candidateRows.push({
          name: actor.meta.name ?? actor.address,
          address: actor.address,
          orderIds: Array.isArray(orderIds) ? orderIds : [],
        });
      }

      candidateRows.sort((left, right) => left.orderIds.length - right.orderIds.length);
      const picked = candidateRows[0];
      const seller = Object.values(actors).find((actor) => actor.address === picked.address);
      assert(seller, 'No seller actor could be selected');

      const sellerFunding = await ensureBalance(api, actors.alice, seller, nex(100));
      note(`seller=${picked.name} existingMarketOrders=${JSON.stringify(picked.orderIds)}`);
      note(`sellerFunding=${JSON.stringify(sellerFunding)}`);

      const priceProtection = codecJson(await (api.query.nexMarket.priceProtectionStore ?? api.query.nexMarket.priceProtection)());
      const marketPrice = Number(readField(priceProtection, 'initialPrice', 'initial_price') ?? 10);
      assert(marketPrice > 0, 'Market initial price should be positive');

      const nextOrderId = Number((await api.query.nexMarket.nextOrderId()).toString());
      const nextTradeId = Number((await api.query.nexMarket.nextUsdtTradeId()).toString());
      note(`marketPrice=${marketPrice} nextOrderId=${nextOrderId} nextTradeId=${nextTradeId}`);

      return {
        sellerName: seller.meta.name ?? seller.address,
        sellerAddress: seller.address,
        marketPrice,
      };
    });

    const sellReservationContext = await runStep(suite, 'place a sell order and finish the reserve-sell trade path', async (note) => {
      const seller = Object.values(actors).find((actor) => actor.address === marketContext.sellerAddress);
      assert(seller, 'Market seller actor could not be resolved');

      const beforeOrderId = Number((await api.query.nexMarket.nextOrderId()).toString());
      const placeSellResult = await submitTx(
        api,
        api.tx.nexMarket.placeSellOrder(nex(10).toString(), marketContext.marketPrice, SELLER_TRON_ADDRESS, null),
        seller,
        'place sell order',
      );
      assert(placeSellResult.ok, `place sell order failed: ${placeSellResult.error ?? 'unknown error'}`);
      expectEvent(placeSellResult, 'nexMarket.OrderCreated', 'place sell order should emit OrderCreated');

      const afterOrderId = Number((await api.query.nexMarket.nextOrderId()).toString());
      assert(afterOrderId === beforeOrderId + 1, `nextOrderId should increase by 1, before=${beforeOrderId} after=${afterOrderId}`);
      const sellOrderId = beforeOrderId;
      const sellOrder = await readMarketOrder(api, sellOrderId);
      const sellOrderStatus = String(readField(sellOrder.human, 'status'));
      assert(sellOrderStatus.toLowerCase().includes('open'), `Sell order should be Open, got ${sellOrderStatus}`);

      const beforeTradeId = Number((await api.query.nexMarket.nextUsdtTradeId()).toString());
      const reserveSellResult = await submitTx(
        api,
        api.tx.nexMarket.reserveSellOrder(sellOrderId, null, BUYER_TRON_ADDRESS),
        actors.alice,
        'reserve sell order',
      );
      assert(reserveSellResult.ok, `reserve sell order failed: ${reserveSellResult.error ?? 'unknown error'}`);
      expectEvent(reserveSellResult, 'nexMarket.UsdtTradeCreated', 'reserve sell order should emit UsdtTradeCreated');
      expectEvent(reserveSellResult, 'nexMarket.BuyerDepositLocked', 'reserve sell order should lock buyer deposit');

      const afterTradeId = Number((await api.query.nexMarket.nextUsdtTradeId()).toString());
      assert(afterTradeId === beforeTradeId + 1, `nextUsdtTradeId should increase by 1, before=${beforeTradeId} after=${afterTradeId}`);
      const sellTradeId = beforeTradeId;
      let trade = await readMarketTrade(api, sellTradeId);
      let tradeStatus = String(readField(trade.human, 'status'));
      assert(tradeStatus.toLowerCase().includes('awaitingpayment'), `Trade should be AwaitingPayment, got ${tradeStatus}`);

      const confirmPaymentResult = await submitTx(
        api,
        api.tx.nexMarket.confirmPayment(sellTradeId),
        actors.alice,
        'confirm payment on reserved sell order',
      );
      assert(confirmPaymentResult.ok, `confirm payment failed: ${confirmPaymentResult.error ?? 'unknown error'}`);
      expectEvent(confirmPaymentResult, 'nexMarket.UsdtPaymentSubmitted', 'confirm payment should emit UsdtPaymentSubmitted');

      const sellerConfirmResult = await submitTx(
        api,
        api.tx.nexMarket.sellerConfirmReceived(sellTradeId),
        seller,
        'seller confirm received on reserved sell order',
      );
      assert(sellerConfirmResult.ok, `seller confirm received failed: ${sellerConfirmResult.error ?? 'unknown error'}`);
      expectEvent(sellerConfirmResult, 'nexMarket.UsdtTradeCompleted', 'seller confirm should emit UsdtTradeCompleted');
      expectEvent(sellerConfirmResult, 'nexMarket.SellerConfirmedReceived', 'seller confirm should emit SellerConfirmedReceived');

      trade = await readMarketTrade(api, sellTradeId);
      tradeStatus = String(readField(trade.human, 'status'));
      assert(tradeStatus.toLowerCase().includes('completed'), `Trade should be Completed, got ${tradeStatus}`);

      note(`sellOrderId=${sellOrderId} sellTradeId=${sellTradeId} finalTradeStatus=${tradeStatus}`);
      return {
        sellOrderId,
        sellTradeId,
      };
    });

    await runStep(suite, 'place a buy order and finish the accept-buy trade path', async (note) => {
      const seller = Object.values(actors).find((actor) => actor.address === marketContext.sellerAddress);
      assert(seller, 'Market seller actor could not be resolved');

      const beforeOrderId = Number((await api.query.nexMarket.nextOrderId()).toString());
      const placeBuyResult = await submitTx(
        api,
        api.tx.nexMarket.placeBuyOrder(nex(10).toString(), marketContext.marketPrice, BUYER_TRON_ADDRESS),
        actors.alice,
        'place buy order',
      );
      assert(placeBuyResult.ok, `place buy order failed: ${placeBuyResult.error ?? 'unknown error'}`);
      expectEvent(placeBuyResult, 'nexMarket.OrderCreated', 'place buy order should emit OrderCreated');

      const afterOrderId = Number((await api.query.nexMarket.nextOrderId()).toString());
      assert(afterOrderId === beforeOrderId + 1, `nextOrderId should increase by 1, before=${beforeOrderId} after=${afterOrderId}`);
      const buyOrderId = beforeOrderId;
      const buyOrder = await readMarketOrder(api, buyOrderId);
      const buyOrderStatus = String(readField(buyOrder.human, 'status'));
      const buyOrderSide = String(readField(buyOrder.human, 'side'));
      assert(buyOrderStatus.toLowerCase().includes('open'), `Buy order should be Open, got ${buyOrderStatus}`);
      assert(buyOrderSide.toLowerCase().includes('buy'), `Order side should be Buy, got ${buyOrderSide}`);

      const beforeTradeId = Number((await api.query.nexMarket.nextUsdtTradeId()).toString());
      const acceptBuyResult = await submitTx(
        api,
        api.tx.nexMarket.acceptBuyOrder(buyOrderId, null, SELLER_TRON_ADDRESS),
        seller,
        'accept buy order',
      );
      assert(acceptBuyResult.ok, `accept buy order failed: ${acceptBuyResult.error ?? 'unknown error'}`);
      expectEvent(acceptBuyResult, 'nexMarket.UsdtTradeCreated', 'accept buy order should emit UsdtTradeCreated');
      expectEvent(acceptBuyResult, 'nexMarket.BuyerDepositLocked', 'accept buy order should lock buyer deposit');

      const afterTradeId = Number((await api.query.nexMarket.nextUsdtTradeId()).toString());
      assert(afterTradeId === beforeTradeId + 1, `nextUsdtTradeId should increase by 1, before=${beforeTradeId} after=${afterTradeId}`);
      const buyTradeId = beforeTradeId;
      let trade = await readMarketTrade(api, buyTradeId);
      let tradeStatus = String(readField(trade.human, 'status'));
      assert(tradeStatus.toLowerCase().includes('awaitingpayment'), `Accepted buy trade should be AwaitingPayment, got ${tradeStatus}`);

      const confirmPaymentResult = await submitTx(
        api,
        api.tx.nexMarket.confirmPayment(buyTradeId),
        actors.alice,
        'confirm payment on accepted buy order',
      );
      assert(confirmPaymentResult.ok, `confirm payment on accepted buy order failed: ${confirmPaymentResult.error ?? 'unknown error'}`);
      expectEvent(confirmPaymentResult, 'nexMarket.UsdtPaymentSubmitted', 'confirm payment should emit UsdtPaymentSubmitted');

      const sellerConfirmResult = await submitTx(
        api,
        api.tx.nexMarket.sellerConfirmReceived(buyTradeId),
        seller,
        'seller confirm received on accepted buy order',
      );
      assert(sellerConfirmResult.ok, `seller confirm received on accepted buy order failed: ${sellerConfirmResult.error ?? 'unknown error'}`);
      expectEvent(sellerConfirmResult, 'nexMarket.UsdtTradeCompleted', 'seller confirm should emit UsdtTradeCompleted');
      expectEvent(sellerConfirmResult, 'nexMarket.SellerConfirmedReceived', 'seller confirm should emit SellerConfirmedReceived');

      trade = await readMarketTrade(api, buyTradeId);
      tradeStatus = String(readField(trade.human, 'status'));
      assert(tradeStatus.toLowerCase().includes('completed'), `Accepted buy trade should be Completed, got ${tradeStatus}`);

      note(`sellFlowTradeId=${sellReservationContext.sellTradeId} buyOrderId=${buyOrderId} buyTradeId=${buyTradeId} finalTradeStatus=${tradeStatus}`);
      return {
        buyOrderId,
        buyTradeId,
      };
    });

    suite.status = suite.status === 'failed' ? 'failed' : 'passed';
  } catch (error) {
    suite.status = 'failed';
    suite.summary.error = error instanceof Error ? error.message : String(error);
    throw error;
  } finally {
    suite.finishedAt = new Date().toISOString();
  }
}

async function main() {
  await fs.mkdir(ARTIFACT_DIR, { recursive: true });

  let api;

  try {
    console.log('[BOOT] waiting for crypto');
    await cryptoWaitReady();
    console.log('[BOOT] connecting api');
    api = await ApiPromise.create({ provider: new WsProvider(WS_URL) });
    console.log('[BOOT] api connected');

    const keyring = new Keyring({ type: 'sr25519', ss58Format: SS58_FORMAT });
    const actors = {
      alice: keyring.addFromUri('//Alice'),
      bob: keyring.addFromUri('//Bob'),
      charlie: keyring.addFromUri('//Charlie'),
      dave: keyring.addFromUri('//Dave'),
      eve: keyring.addFromUri('//Eve'),
      ferdie: keyring.addFromUri('//Ferdie'),
    };

    console.log('[BOOT] capturing environment');
    report.environment = await captureEnvironment(api, actors);
    await writeJson(ENVIRONMENT_PATH, report.environment);
    console.log('[BOOT] environment captured');

    await runEntityExtensionSuite(api, actors);
    await runNexMarketTradeSuite(api, actors);

    const passedSuites = report.suites.filter((suite) => suite.status === 'passed').length;
    report.summary = {
      status: passedSuites === report.suites.length ? 'passed' : 'failed',
      totalSuites: report.suites.length,
      passedSuites,
      failedSuites: report.suites.length - passedSuites,
    };
  } catch (error) {
    report.summary = {
      status: 'failed',
      error: error instanceof Error ? error.message : String(error),
    };
    throw error;
  } finally {
    report.finishedAt = new Date().toISOString();
    await writeJson(RESULTS_PATH, report);
    if (api) {
      await api.disconnect();
    }
  }
}

main().catch((error) => {
  console.error(`Remote business flow run failed: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
});
