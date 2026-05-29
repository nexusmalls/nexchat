import { ApiPromise, WsProvider } from "@polkadot/api";

const addr = "X4TR7tBwyFoMZTcgqFhw44LUkMoN96pgf3q7NnxU5X6HzYgXk";
const provider = new WsProvider("wss://rpc.nexcommunity.net");
const api = await ApiPromise.create({ provider });

try {
  const [bonded, validatorPrefs, sessionValidators, queuedKeys, currentIndex, activeEra] = await Promise.all([
    api.query.staking.bonded(addr),
    api.query.staking.validators(addr),
    api.query.session.validators(),
    api.query.session.queuedKeys(),
    api.query.session.currentIndex(),
    api.query.staking.activeEra(),
  ]);

  const controller = bonded.isSome ? bonded.unwrap().toString() : null;
  const ledger = controller ? await api.query.staking.ledger(controller) : null;
  const erasStakersOverview = activeEra.isSome
    ? await api.query.staking.erasStakersOverview(activeEra.unwrap().index, addr)
    : null;

  const result = {
    address: addr,
    bondedController: controller,
    ledger: ledger && !ledger.isNone ? ledger.toHuman() : null,
    validatorPrefs: validatorPrefs && !validatorPrefs.isEmpty ? validatorPrefs.toHuman() : null,
    inSessionValidators: sessionValidators.map((v) => v.toString()).includes(addr),
    sessionValidatorIndex: sessionValidators.map((v) => v.toString()).indexOf(addr),
    sessionValidatorCount: sessionValidators.length,
    hasQueuedKeys: queuedKeys.some(([account]) => account.toString() === addr),
    queuedKeyEntry: queuedKeys.find(([account]) => account.toString() === addr)?.[1].toHuman() ?? null,
    currentSessionIndex: currentIndex.toString(),
    activeEra: activeEra.isSome ? activeEra.unwrap().index.toString() : null,
    activeEraStakerOverview:
      erasStakersOverview && !erasStakersOverview.isEmpty ? erasStakersOverview.toHuman() : null,
  };

  console.log(JSON.stringify(result, null, 2));
} finally {
  await api.disconnect();
}
