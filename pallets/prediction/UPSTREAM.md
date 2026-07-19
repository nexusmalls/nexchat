# Prediction Port Upstream Manifest

## Fixed baselines

- Nexus repository: `d14dd20b6efc1898801d9c632be27923f88d2087`
- Nexus branch at import: `feat/hyperbridge-integration`
- Zeitgeist repository: `https://github.com/zeitgeistpm/zeitgeist`
- Zeitgeist commit: `39ad8d60aa2f7af0a465d58c5e87dcc509602df5`
- ORML repository: `https://github.com/open-web3-stack/open-runtime-module-library`
- ORML branch: `polkadot-stable2512`
- ORML commit: `f389cbdb1d37f4113a3784d72542aa080beb299c`
- HydraDX repository: `https://github.com/galacticcouncil/HydraDX-node`
- HydraDX tag: `v37.0.0`
- HydraDX commit: `fcedfa7580cfa9ce4878d799bc4ab4eb917f8d8e`

The Nexus working tree already contained unrelated Hyperbridge, USDX, runtime, and
commission changes when Phase 0 started. The commit above identifies the common
repository base; Phase 0 changes must be reviewed by path rather than treating the
entire working-tree diff as prediction work.

## Imported paths

| Nexus path | Upstream path | Phase 0 state |
|---|---|---|
| `pallets/prediction/macros` | `zeitgeist/macros` | Verbatim import |
| `pallets/prediction/primitives` | `zeitgeist/primitives` | Imported; FRAME 45 codec adaptation started |
| `pallets/prediction/market-commons` | `zeitgeist/zrml/market-commons` | Verbatim import |
| `pallets/prediction/authorized` | `zeitgeist/zrml/authorized` | Phase 1 FRAME 45 port; not runtime-wired |
| `pallets/prediction/global-disputes` | `zeitgeist/zrml/global-disputes` | Phase 1 FRAME 45 port; not runtime-wired |
| `pallets/prediction/court` | `zeitgeist/zrml/court` | Phase 1 FRAME 45 port; not runtime-wired |
| `pallets/prediction/prediction-markets` | `zeitgeist/zrml/prediction-markets` | Phase 1 FRAME 45 source/mock port; not runtime-wired |
| `pallets/prediction/prediction-markets/runtime-api` | `zeitgeist/zrml/prediction-markets/runtime-api` | Verbatim API surface with Nexus docs; not runtime-wired |
| `pallets/prediction/combinatorial-tokens` | `zeitgeist/zrml/combinatorial-tokens` | Phase 1 FRAME 45 source/mock/fuzz port; not runtime-wired |
| `pallets/prediction/orderbook` | `zeitgeist/zrml/orderbook` | Phase 1 FRAME 45 source/mock/fuzz port; not runtime-wired |
| `pallets/prediction/parimutuel` | `zeitgeist/zrml/parimutuel` | Phase 1 FRAME 45 source/mock port; not runtime-wired |
| `pallets/prediction/styx` | `zeitgeist/zrml/styx` | Phase 1 FRAME 45 source/mock/benchmark port; not runtime-wired |
| `pallets/prediction/neo-swaps` | `zeitgeist/zrml/neo-swaps` | Phase 1 FRAME 45 source/mock/benchmark/fuzz port; not runtime-wired |
| `pallets/prediction/hybrid-router` | `zeitgeist/zrml/hybrid-router` | Phase 1 FRAME 45 source/mock/benchmark/test port; not runtime-wired |
| `pallets/prediction/swaps` | `zeitgeist/zrml/swaps` | Phase 1 FRAME 45 source/mock/benchmark/migration/test port; not runtime-wired |
| `pallets/prediction/swaps/runtime-api` | `zeitgeist/zrml/swaps/runtime-api` | Phase 1 runtime API port for compile/mock verification; not runtime-wired |
| `pallets/prediction/swaps/rpc` | `zeitgeist/zrml/swaps/rpc` | Phase 1 host-only RPC port; not node-wired |
| `pallets/prediction/swaps/fuzz` | `zeitgeist/zrml/swaps/fuzz` | Phase 1 fuzz-target port |
| `pallets/prediction/futarchy` | `zeitgeist/zrml/futarchy` | Phase 1 FRAME 45 source/mock/benchmark/test port; not runtime-wired |
| `pallets/prediction/futarchy/fuzz` | `zeitgeist/zrml/futarchy/fuzz` | Phase 1 fuzz-target port |
| `pallets/prediction/vendor/orml/currencies` | `orml/currencies` | Verbatim stable2512 vendor |
| `pallets/prediction/vendor/orml/tokens` | `orml/tokens` | Verbatim stable2512 vendor |
| `pallets/prediction/vendor/orml/traits` | `orml/traits` | Verbatim stable2512 vendor |
| `pallets/prediction/vendor/orml/utilities` | `orml/utilities` | Verbatim stable2512 vendor |
| `pallets/prediction/vendor/hydra-dx-math` | `HydraDX-node/math` | v37 source; dependencies rebound to Nexus workspace |
| `pallets/prediction/smoke` | Nexus-only | Phase 0 dependency and asset POC |
| `pallets/prediction/differential` | Nexus-only | Phase 3 behavioral differential baseline harness |
| `pallets/prediction/mock` | Nexus-only | Phase 1 shared mock helpers and ORML/assets base runtime |
| `pallets/prediction/control` | Nexus-only | Phase 2 batch 1 governance gates and static call registry; not runtime-wired |
| `pallets/prediction/collateral` | Nexus-only | Phase 2 batch 2 safe pallet-assets/ORML mirror; not runtime-wired |

The ORML `utilities` crate is part of the minimum transitive source closure required
by `orml-currencies` and `orml-traits`. No XCM execution pallet, asset registry,
unknown-tokens pallet, or XTokens pallet is vendored.

## Local patch classification

Patch categories must remain separable:

1. `SDK`: FRAME/SP/codec API adaptations required to compile on Nexus.
2. `ASSET`: Nexus native, outcome, and foreign-collateral semantics.
3. `RUNTIME`: Nexus Config, origin, index, weights, and API wiring.
4. `NAMING`: ZTG-to-NEX naming changes after the differential baseline.
5. `BUGFIX`: Independently justified correctness or security fixes.

Phase 0 currently contains one upstream source patch:

- `SDK`: derive `DecodeWithMemTracking` for `Asset` and `ScalarPosition`, required
  because FRAME 45 `Parameter` requires memory-tracked decoding.
- `SDK`: resolve the vendored HydraDX math crate's workspace dependencies from the
  Nexus FRAME/SP graph instead of its original stable2409 workspace.

Phase 1 adds asset-model and mock-runtime patches:

- `ASSET`: `BlockNumber = u32`, `ForeignAsset(u64)`, upstream `u32` fixture helpers.
- `SDK`: FRAME 45 mock migration for `zrml-market-commons` (`derive_impl`,
  `ExtensionsWeightInfo`, `DoneSlashHandler`, `dev_accounts`).
- `SDK`: derive `DecodeWithMemTracking` for `OutcomeReport`, use `MockBlockU32`,
  and migrate `zrml-authorized` Config/mock APIs to FRAME 45.
- `SDK`: derive `DecodeWithMemTracking` for global-disputes storage types, use
  `MockBlockU32` and FRAME 45 `derive_impl`, and adapt the reserve-lock test to
  FRAME 45 balances semantics.
- `SDK`: preserve court logic and benchmarks while adding memory-tracked codec
  derives, `MockBlockU32`, FRAME 45 mock preludes, deterministic mock randomness,
  and the treasury block-number provider.
- `SDK`: adapt prediction-markets transfer signatures, FRAME 45 mock configs,
  `MockBlockU32`, treasury block-number provider, deterministic randomness, and
  memory-tracked call/event primitive types.
- `ASSET`: permanently remove prediction-markets parachain/XCM/asset-registry
  paths and admit only native or explicitly policy-whitelisted foreign base assets.
- `RUNTIME`: keep prediction-markets and its runtime API out of production runtime
  wiring; the runtime API crate is workspace-registered for compile verification only.
- `SDK`: preserve combinatorial-token business logic, tests, benchmarks, and fuzz
  surfaces while adding memory-tracked call/error types, FRAME 45 ORML transfer
  requirements, `MockBlockU32`, `derive_impl`, and balances `dev_accounts: None`.
- `ASSET`: permanently remove combinatorial-token parachain and ORML asset-registry
  mock/manifest paths. Market fixtures admit native or the shared static USDX id only;
  Phase 2 must validate live asset existence, mirror validity, and pause/freeze state
  at market creation because combinatorial tokens only consume existing market records.
- `RUNTIME`: keep combinatorial tokens out of production runtime wiring.
- `SDK`: preserve orderbook logic, tests, benchmarks, migrations, weights, and fuzz
  API while adding memory-tracked codec derives, FRAME 45 ORML transfer
  requirements, `MockBlockU32`, `derive_impl`, and balances `dev_accounts: None`.
- `ASSET`: orderbook consumes existing market records and does not independently
  admit collateral. Its mock references the shared native-or-USDX static policy;
  foreign-collateral validation remains enforced at market creation.
- `RUNTIME`: keep orderbook out of production runtime wiring.
- `SDK`: preserve parimutuel betting, payout, refund, benchmark, storage-version,
  and generated-weight behavior while adding memory-tracked error decoding,
  FRAME 45 ORML transfer requirements, `MockBlockU32`, `derive_impl`, and balances
  `dev_accounts: None`.
- `ASSET`: parimutuel consumes existing market records and does not independently
  admit collateral. Its mock references the shared native-or-USDX static policy;
  foreign-collateral validation remains enforced at market creation.
- `RUNTIME`: keep parimutuel out of production runtime wiring.
- `SDK`: preserve Styx burn/cross business logic, tests, benchmarks, and generated
  weights while adopting `MockBlockU32`, FRAME 45 `derive_impl`, and balances
  `dev_accounts: None`. No additional `DecodeWithMemTracking` derive is required:
  the pallet introduces no custom SCALE-decoded call, event, or error payload type.
- `ASSET`: no Styx asset-model patch. The upstream `Currency` abstraction, `ZTG`
  terminology, `Balance`, and `BASE`-based default burn amount remain unchanged.
- `RUNTIME`: keep Styx out of production runtime wiring.
- `SDK`: preserve Neo Swaps LMSR and combinatorial math, liquidity tree, migrations,
  generated weights, tests, benchmarks, and fuzz surfaces while adding FRAME 45
  memory-tracked error decoding, ORML transfer existence requirements, `MockBlockU32`,
  `derive_impl`, balances `dev_accounts: None`, treasury `BlockNumberProvider`, and
  deterministic mock randomness.
- `ASSET`: permanently remove Neo Swaps XCM, pallet-XCM, XCM-builder, parachain, and
  ORML asset-registry manifest/mock/test branches. Its market-creation dependency uses
  the shared explicit native-or-USDX static policy; live foreign-collateral existence,
  mirror, pause, and freeze validation remains deferred to Phase 2.
- `RUNTIME`: keep Neo Swaps out of production runtime wiring.
- `SDK`: preserve Hybrid Router routing logic, buy/sell tests, benchmarks, generated
  weights, types, and utilities while adding FRAME 45 memory-tracked call/event
  decoding, ORML transfer existence requirements, `MockBlockU32`, `derive_impl`,
  balances `dev_accounts: None`, treasury `BlockNumberProvider`, and deterministic
  mock randomness.
- `ASSET`: permanently remove Hybrid Router parachain, XCM, pallet-XCM, XCM-builder,
  collective-flip, and ORML asset-registry manifest/mock/test branches. `Asset::Ztg`
  and all upstream routing fixtures remain unchanged; the mock references the shared
  native-or-USDX policy without expanding production asset admission.
- `RUNTIME`: keep Hybrid Router out of the production runtime. Its per-crate
  `construct_runtime!` remains because the router tests require one coherent graph
  spanning market commons, prediction markets, combinatorial tokens, Neo Swaps,
  orderbook, and dispute pallets; the shared mock crate only supplies reusable asset
  policy and base components, so replacing this graph would require structural
  refactoring.
- `SDK`: preserve legacy Swaps CPMM math, migrations, generated weights, benchmarks,
  tests, runtime API, RPC, and fuzz targets while adding FRAME 45 memory-tracked
  decoding, ORML transfer existence requirements, `MockBlockU32`, `derive_impl`,
  balances `dev_accounts: None`, and current `sp-api`/jsonrpsee signature adaptations.
- `ASSET`: no Swaps asset-model patch. Upstream `Asset::Ztg`, pool-share identifiers,
  constants, and mock asset semantics remain unchanged; parachain/XCM dependencies
  are not part of this upstream subtree or its Nexus manifests.
- `RUNTIME`: keep Swaps and its runtime API out of the production runtime, and keep
  its RPC out of `node/src/rpc.rs`; workspace registration provides isolated checks.
- `SDK`: preserve Futarchy proposal storage, oracle evaluation, anonymous scheduler
  boundary, generated weights, tests, benchmarks, and fuzz target while adding
  FRAME 45 memory-tracked decoding, `MockBlockU32`, `derive_impl`, balances
  `dev_accounts: None`, and the current schedule v3 `Anon`/`Bounded` call API.
- `ASSET`: Futarchy remains generic over its oracle and does not change `Asset::Ztg`.
  The upstream mock-only parachain asset-registry fixture is omitted because that
  registry is outside the Nexus Phase 1 dependency closure; no production pallet
  logic or accepted asset semantics are changed.
- `RUNTIME`: keep Futarchy and a production Scheduler implementation out of the
  production runtime; workspace registration provides isolated compile verification.
- `RUNTIME`: Nexus-only `prediction-mock-runtime` shared ORML/assets mock base; per-pallet
  `Config` impls remain in each crate's `mock.rs`.

Phase 2 batch 1 adds control-boundary patches only:

- `ASSET`: move the generic `PredictionBaseAssetPolicy<AssetId>` boundary from
  prediction-markets into `zeitgeist-primitives::traits`; the shared mock policy
  implements that single trait directly.
- `RUNTIME`: add Nexus-only `pallet-prediction-control` with inert defaults
  (`Disabled`, every module disabled), governance updates, pure gate evaluation,
  and a source-audited registry of 68 dispatchables across 12 business modules.
- `BUGFIX`: classify Neo Swaps `combo_sell` as `RiskIncreasing`, not `Unwind`,
  because its buy legs can increase pool exposure. Registry tests pin this and
  other security-sensitive classifications and exercise all 68 entries against
  the complete mode/module filter matrix.
- `RUNTIME`: keep prediction-control out of production runtime/node wiring.
  Runtime-call filtering, control-call self-exemption, and cross-module gates for
  PredictionMarkets pool deployment and HybridRouter remain Phase 6 work.
- `RUNTIME`: Phase 2 control weights are explicitly non-production estimates;
  Phase 7 must generate and review benchmark weights before activation.

Phase 2 batch 2 adds the collateral adapter only:

- `ASSET`: add a transactional 1:1 mirror between whitelisted `pallet-assets`
  `u64` assets and ORML `Asset::ForeignAsset(u64)`, with exact issuance/escrow
  checks before and after each mutation and no duplicated user-balance storage.
- `ASSET`: require `Full` mode, whitelist admission, deposit pause gates, a
  runtime-provided live `AssetValidator`, and a consistent mirror for deposits.
  Withdrawals remain available across control, whitelist, pause, and validator
  changes while retaining ORML liquidity restrictions.
- `ASSET`: implement the shared `PredictionBaseAssetPolicy<u64>` from the same
  live gates. No XCM, asset registry, Entity-id range shortcut, or direct
  cross-pallet storage read is introduced.
- `RUNTIME`: keep prediction-collateral out of production runtime/node wiring.
  The production validator, including USDX protocol/PSM readiness, remains a
  Phase 6 adapter.
- `RUNTIME`: Phase 2 collateral weights are explicitly non-production estimates;
  Phase 7 must generate and review benchmark weights before activation.
- `TEST`: cover empty-safe defaults, governance origins, native-ledger isolation,
  deposit/withdraw conservation and rollback, pauses, validation failures,
  liquidity restrictions, desynchronization, whitelist removal, and repeated
  multi-user sequences.

Phase 2 batch 3 integrates that adapter only into the Prediction Markets mock:

- `ASSET`: replace Prediction Markets' static mock base-asset policy with
  `pallet-prediction-collateral`; require a real FRAME Assets entry whose
  public `AssetDetails.status` is `Live`.
- `TEST`: remove direct ORML ForeignAsset genesis balances. Force-create and
  mint USDX in FRAME Assets, retain the one-unit minimum required by
  `Preservation::Preserve`, and issue the unchanged `INITIAL_BALANCE` fixture
  only through collateral deposits.
- `TEST`: verify Native and approved USDX market creation, all live collateral
  gates including a real FRAME Assets Frozen/Live transition, recovery after
  gate removal, and mirror issuance/escrow conservation across ordinary ORML
  transfers.
- `ASSET`: validate an asset when governance enables whitelist admission;
  disabling admission remains unconditional so invalid assets can always be
  removed.
- `RUNTIME`: no production validator, runtime/node wiring, or generated
  production weights are included. USDX protocol/PSM readiness and broader
  Neo Swaps/Hybrid Router integration remain Phase 6 work. Phase 2 final
  development closure was declared only after focused tests, all 13 imported
  pallet suites, runtime-valid WASM no-std checks, benchmark feature checks,
  formatting, and strict linting of the two new pallets passed. This is not a
  runtime-readiness or production-candidate claim.

Phase 3 closes the isolated market and dispute core:

- `TEST`: execute the five fixed-upstream suites (373 tests) and the matching
  Nexus suites (376 tests). The three additional Nexus Prediction Markets tests
  cover the collateralized foreign-asset fixture, live admission policy, and
  foreign-asset edit flow.
- `TEST`: extend the existing Court-to-GlobalDisputes integration scenario
  through voting, automatic resolution, final market state, queue cleanup,
  Court mapping cleanup, and appeal-bond conservation.
- `BUGFIX`: retain escalated `CourtInfo` after draws are removed so the final
  GlobalDisputes outcome can still drive `Court::exchange`. Remove the retained
  Court and both mappings atomically after appeal bonds are settled. The fixed
  upstream test expected immediate Court deletion and did not execute the final
  resolution, which otherwise fails with `CourtNotFound`.
- `RUNTIME`: keep all five pallets out of production runtime/node wiring.
  The additional cleanup writes use non-production imported weights; Phase 7
  must regenerate them before activation.

Phase 3 differential baseline adds the first behavioral parity harness:

- `TEST`: add Nexus-only `prediction-differential` with normalized snapshot
  comparison for five native lifecycle scenarios pinned against upstream commit
  `39ad8d60`, including Court escalation through GlobalDisputes to automatic
  resolution and appeal-bond cleanup.
- `TEST`: add prediction-markets complete-set buy/sell roundtrip conservation
  property tests for native and USDX collateral.
- `RUNTIME`: no production runtime wiring. Upstream-side runner automation
  remains follow-up work before the `Ztg -> Native` rename PR.

Phase 6 wires the complete subsystem into the production runtime in an inert state:

- `RUNTIME`: reserve and register pallet indices 176–192 without changing the
  existing 0–175 assignments; keep the global mode `Disabled`, every module
  disabled, and the collateral whitelist empty.
- `RUNTIME`: add Nexus origins, sovereign accounts, ORML currency adapters,
  conservative non-unit weight providers, BABE-backed Court randomness, the
  runtime call filter, and a bounded upgrade marker that rejects unsafe initial
  prediction storage.
- `ASSET`: wire the live `pallet-assets` validator. USDX admission additionally
  requires the canonical protocol asset, an unpaused PSM with a non-zero global
  debt ceiling, and issuance equal to PSM debt; Phase 6 defaults therefore reject
  USDX deposits.
- `RUNTIME`: register all prediction pallets that currently expose FRAME
  benchmarks. `control`, `collateral`, and `orml-currencies` have no benchmark
  implementation yet; their Phase 7 benchmark work remains explicit.
- `RUNTIME`: expose upstream `SwapsApi` and `PredictionMarketsApi`, plus the
  Nexus-only bounded `PredictionViewApi`. Merge the upstream swaps RPC and
  `prediction_*` read-only RPC methods into the node.
- `RUNTIME`: imported and Phase 2 weights remain integration-only. No module may
  be enabled before Phase 7 generates and reviews Nexus production weights.

## Future upstream sync order

1. Re-import the selected upstream commit into a clean temporary tree.
2. Apply SDK patches.
3. Apply asset-model patches.
4. Apply runtime integration patches.
5. Apply naming patches.
6. Apply independently documented bug fixes.
7. Run SCALE golden tests, differential tests, and the no-std smoke target.

Never merge a new upstream snapshot directly over Nexus runtime wiring.
