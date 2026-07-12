# Prediction Phase 1 Primitives and Mock Runtime Report

Date: 2026-07-12

## Scope

Phase 1 goal: all prediction business crates compile on the Nexus FRAME 45 SDK and
upstream mock tests can start running, without wiring into the production runtime.

## Type alignment

| Type | Upstream Zeitgeist | Nexus Phase 1 |
|---|---|---|
| `BlockNumber` | `u64` | `u32` (matches Nexus runtime) |
| `ForeignAsset` payload | `u32` | `u64` (`pallet-assets` `AssetId`) |
| `Asset::Ztg` name | kept | kept until Phase 3 differential baseline |
| Variant order | unchanged | unchanged |

Helpers added in `primitives/src/asset.rs`:

- `foreign_asset_from_upstream_id(u32) -> u64`
- `foreign_asset_from_upstream<MarketId>(u32) -> Asset<MarketId>`

Mock constant `MaxMarketLifetime` was reduced from `100_000_000_000` to `4_000_000_000`
so it fits `u32`.

## SCALE golden tests

Added `pallets/prediction/primitives/src/scale_golden.rs` covering:

- locked `Asset` variant discriminants (`0..=7`)
- stable `Asset::Ztg` encoding (`[4]`)
- upstream `ForeignAsset(u32)` fixture `900_000` zero-extends to Nexus `u64`
- intentional encoding difference between upstream `u32` and Nexus `u64` payloads
- `BlockNumber` width (`u32`, 4-byte SCALE encoding)
- roundtrips for categorical, pool, and combinatorial assets

## Shared mock runtime

New crate: `pallets/prediction/mock` (`prediction-mock-runtime`).

Provides:

- FRAME 45 `derive_impl` wiring for System, Balances, Timestamp, ORML tokens/currencies,
  and `pallet-assets`
- explicit `MockBaseAssetPolicy` whitelist (`USDX_ASSET_ID = 900_000` only)
- `ExtBuilder` with optional USDX seeding

Each pallet keeps its own `construct_runtime!` extension and local `Config` impl.
`zrml-market-commons` mock was migrated to FRAME 45 (`ExtensionsWeightInfo`,
`DoneSlashHandler`, `dev_accounts`) and consumes the shared base-asset policy crate.

## Verification

The following commands pass:

```bash
cargo test -p zeitgeist-primitives
cargo test -p prediction-mock-runtime
cargo test -p zrml-market-commons
cargo test -p prediction-phase0-smoke
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zeitgeist-primitives \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-market-commons \
  --no-default-features --target wasm32-unknown-unknown
```

Results:

- `zeitgeist-primitives`: all unit tests including 10 SCALE golden vectors
- `zrml-market-commons`: 19/19 upstream mock tests
- `prediction-phase0-smoke`: 2/2 asset POC tests
- `prediction-mock-runtime`: base-asset whitelist test

## L2 progress

`zrml-authorized` is now imported and adapted to FRAME 45:

- uses `MockBlockU32`, matching Nexus `BlockNumber = u32`
- uses FRAME 45 config preludes and genesis `dev_accounts`
- `OutcomeReport` now derives `DecodeWithMemTracking`
- production runtime wiring remains intentionally absent
- upstream benchmark weights are compile-only and must be regenerated on Nexus

Verification:

```bash
cargo test -p zrml-authorized
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-authorized \
  --no-default-features --target wasm32-unknown-unknown
cargo check -p zrml-authorized --features runtime-benchmarks
```

The full upstream suite is preserved: 13 business tests plus 2 FRAME-generated
mock-runtime tests pass by default (15 total), and all 9 upstream benchmark
scenarios execute successfully with `runtime-benchmarks` enabled (24 total
tests). The upstream `outcomes` storage getter is also retained.

## Global disputes progress

`zrml-global-disputes` is now imported and adapted to FRAME 45 with
`MockBlockU32`, `derive_impl`, memory-tracked codec derives, and no production
runtime wiring. The upstream business logic is unchanged; the reserve-lock test
reflects FRAME 45 balances semantics, where reserving funds does not consult
transfer locks.

Verification covers all 42 pallet tests, wasm `no_std`, and
`runtime-benchmarks` compilation.

## Next (Phase 1 continuation)

The remaining L2 crate, `zrml-court`, is now imported and adapted to FRAME 45:

- production randomness remains generic; the test runtime uses deterministic
  subject hashing and does not depend on collective-flip
- the mock uses `MockBlockU32`, FRAME 45 `derive_impl`, balances
  `dev_accounts: None`, and the treasury block-number provider
- court call/event types derive memory-tracked decoding where FRAME 45 requires it
- all 118 upstream unit tests pass; wasm `no_std` and benchmark compilation pass

Phase 1 L2 source ports (`authorized`, `global-disputes`, and `court`) are complete.

## Prediction markets progress

`zrml-prediction-markets` and `zrml-prediction-markets-runtime-api` are now
workspace-registered without production runtime wiring. The full upstream source,
tests, and benchmarks are preserved with FRAME 45 transfer/mock adaptations and
`MockBlockU32`.

The parachain, XCM, insecure-randomness, and ORML asset-registry paths were removed.
The mock uses deterministic randomness and the shared static collateral policy:
native is allowed, USDX fixture `900_000` is the only allowed `ForeignAsset`, and
outcome/pool-share assets plus all other foreign ids are rejected. This is a
compile-safe Phase 1 admission adapter only; Phase 2 must still enforce live
`pallet-assets` existence, mirror validity, and pause/freeze state.

Verification covers 181 default tests, WASM `no_std`, runtime benchmarks, and the
runtime API crate.

## Combinatorial tokens progress

`zrml-combinatorial-tokens` is workspace-registered without production runtime
wiring. The upstream split, merge, redeem, cryptographic ID, benchmark, and fuzz
surfaces are retained. FRAME 45 adaptations add ORML existence requirements,
`DecodeWithMemTracking`, `MockBlockU32`, `derive_impl`, and balances
`dev_accounts: None`; parachain and ORML asset-registry paths are removed.

The mock market boundary admits native collateral or the shared static USDX
fixture `900_000`. The pallet itself only consumes existing market records, so
Phase 2 must enforce live `pallet-assets` existence, mirror validity, and
pause/freeze state when a market is created; this deferred check is not replaced
by permissive admission.

All 751 default tests pass. Enabling `runtime-benchmarks` executes 759 tests,
including all 8 upstream benchmark scenarios. WASM `no_std` and benchmark
compilation also pass, and all three retained cargo-fuzz binaries compile.

## Orderbook progress

`zrml-orderbook` is workspace-registered without production runtime wiring. The
upstream order placement, partial/full fill, cancellation, external-fee logic,
tests, benchmarks, empty migration surface, generated weights, and fuzz target
are retained. FRAME 45 adaptations use ORML currencies/tokens,
`DecodeWithMemTracking`, `MockBlockU32`, `derive_impl`, explicit transfer
existence requirements, and balances `dev_accounts: None`.

The orderbook consumes existing market records and does not create or admit
collateral. Its mock references the shared native-or-USDX static policy; live
foreign-collateral existence, mirror, and pause/freeze validation remains at the
market-creation boundary and is not weakened here.

All 34 default tests pass. Enabling `runtime-benchmarks` executes 37 tests,
including all 3 upstream benchmark scenarios. WASM `no_std`, benchmark
compilation, and the retained cargo-fuzz binary also compile.

## Parimutuel progress

`zrml-parimutuel` is workspace-registered without production runtime wiring.
The upstream categorical betting, fee, proportional payout, no-winner refund,
benchmark, storage-version, and generated-weight behavior are retained. FRAME 45
adaptations use ORML currencies/tokens, `DecodeWithMemTracking`, `MockBlockU32`,
`derive_impl`, explicit transfer existence requirements, and balances
`dev_accounts: None`.

Parimutuel consumes existing market records and does not independently admit
collateral. Its mock references the shared native-or-USDX static policy; live
foreign-collateral existence, mirror, and pause/freeze validation remains at the
market-creation boundary.

The tracked upstream `tests/assets.rs` collateral-lifecycle draft is retained
but, as in the selected upstream snapshot, is not registered by `tests/mod.rs`;
its asset creator/destroyer integration belongs to the Phase 2 asset layer.

All 40 default tests pass. Enabling `runtime-benchmarks` executes 43 tests,
including all 3 upstream benchmark scenarios. WASM `no_std`, benchmark
compilation, and scoped strict clippy also pass.

## Neo Swaps progress

`zrml-neo-swaps` and its three upstream fuzz binaries are workspace-registered
without production runtime wiring. The upstream LMSR and combinatorial math,
liquidity tree, trading and liquidity operations, futarchy oracle, migrations,
generated weights, tests, benchmarks, and fuzz surfaces are retained.

FRAME 45 adaptations use ORML currencies/tokens with explicit transfer existence
requirements, `DecodeWithMemTracking` for pallet error payloads, `MockBlockU32`,
`derive_impl`, balances `dev_accounts: None`, treasury `BlockNumberProvider`, and
deterministic mock randomness. XCM, pallet-XCM, XCM-builder, parachain, insecure
randomness, and ORML asset-registry paths are permanently absent.

The mock prediction-market boundary admits native collateral or the shared static
USDX fixture `900_000`; it does not use a permissive base-asset filter. Live
foreign-collateral existence, mirror, pause, and freeze validation remains safely
deferred to the Phase 2 market-creation boundary.

Do not wire prediction pallets into `runtime/src/lib.rs` until Phase 2 asset layer
(`PredictionCollateral`, `PredictionControl`) is ready.

## Final Phase 1 closure verification

Verification date: 2026-07-12 (UTC+8).

This section is the authoritative final result for Phase 1 and supersedes the
incremental counts above. All checks were run in the existing Nexus working tree;
no production runtime or node RPC wiring was added.

### Workspace and source inventory

All 13 Zeitgeist business pallets are workspace members and have their own
`src/lib.rs`:

| Pallet crate | Host test command feature | Tests | WASM `no_std` |
|---|---:|---:|---:|
| `zrml-market-commons` | default | 19 | pass |
| `zrml-authorized` | default | 15 | pass |
| `zrml-court` | default | 118 | pass |
| `zrml-global-disputes` | default | 42 | pass |
| `zrml-prediction-markets` | `mock` | 181 | pass |
| `zrml-combinatorial-tokens` | `mock` | 751 | pass |
| `zrml-swaps` | `mock` | 108 | pass |
| `zrml-neo-swaps` | `mock` | 436 | pass |
| `zrml-orderbook` | `mock` | 34 | pass |
| `zrml-parimutuel` | `mock` | 40 | pass |
| `zrml-hybrid-router` | `mock` | 46 | pass |
| `zrml-futarchy` | `mock` | 7 | pass |
| `zrml-styx` | default | 8 | pass |
| **Total** |  | **1,805** | **13/13 pass** |

The retained adjunct crate inventory is also coherent with the imported Phase 1
surface:

- runtime APIs: `zrml-prediction-markets-runtime-api`,
  `zrml-swaps-runtime-api`
- host RPC: `zrml-swaps-rpc`
- fuzz crates: combinatorial tokens (3 binaries), Neo Swaps (3), orderbook (1),
  Swaps (9), and Futarchy (1)
- Nexus support crates: `zeitgeist-primitives`, `zeitgeist-macros`,
  `prediction-phase0-smoke`, and `prediction-mock-runtime`

Prediction Markets full-workflow fuzzing is still a Phase 7 deliverable; its
absence from this Phase 1 source closure does not weaken the Phase 1 exit gates.

### Commands and results

Every pallet passed the runtime-valid WASM gate (the plain host-only
`cargo check --no-default-features` form is intentionally not used):

```bash
RUSTFLAGS="--cfg substrate_runtime" cargo check -p <each-of-13-crates> \
  --no-default-features --target wasm32-unknown-unknown
```

The upstream mock suites were actually executed, not merely compiled:

```bash
cargo test -p <crate> --lib
cargo test -p <crate-requiring-it> --lib --features mock
```

The 13 pallet suites passed 1,805 tests. Supporting verification also passed:

```bash
cargo test -p zeitgeist-primitives --lib
cargo test -p zeitgeist-primitives --lib scale_golden
cargo test -p prediction-phase0-smoke --lib
cargo test -p prediction-mock-runtime --lib
```

Results were 633/633 primitives tests (including the 10/10 SCALE golden subset),
4/4 Phase 0 smoke tests, and 3/3 shared-mock tests. Including the 13 pallet
suites, 2,445 host tests ran successfully.

The four most recently completed pallets passed benchmark-feature compilation:

```bash
cargo check -p zrml-styx --features runtime-benchmarks
cargo check -p zrml-hybrid-router --features runtime-benchmarks
cargo check -p zrml-swaps --features runtime-benchmarks
cargo check -p zrml-futarchy --features runtime-benchmarks
```

The retained Swaps and Futarchy adjuncts also compile:

```bash
cargo check -p zrml-swaps-runtime-api
cargo check -p zrml-swaps-rpc
cargo check -p zrml-swaps-fuzz
cargo check -p zrml-futarchy-fuzz
```

`cargo tree -d` plus exact package-source inspection confirms one core SDK graph:

```text
frame-support       45.1.3
frame-system        45.0.0
sp-runtime          45.0.0
sp-core             39.0.0
sp-io               44.0.0
parity-scale-codec   3.7.5
scale-info           2.11.6
```

Each core package resolves once from crates.io; there is no second FRAME/SP or
SCALE type graph. Ordinary non-core duplicate dependencies reported by
`cargo tree -d` are not a Phase 0/1 failure.

Repository hygiene checks also pass:

```bash
cargo fmt --all --check
git diff --check
```

Searches of `runtime/` and `node/src/rpc.rs` find no Prediction, `zrml-*`, or
Swaps API/RPC wiring. Existing unrelated working-tree changes in those areas were
left untouched.

### Shared mock boundary and remaining limits

`prediction-mock-runtime` deliberately shares policy and base components:
System, Balances, Timestamp, ORML tokens/currencies, assets fixtures,
`ExtBuilder`, and the explicit native-or-USDX base-asset policy. It does **not**
share one full `construct_runtime!` for every pallet. Each business crate keeps
its own coherent test `Runtime` and local `Config` implementations because its
upstream mock graph differs; replacing those per-crate runtimes would be a
business-test refactor, not a Phase 1 compatibility adaptation.

Known limits remain outside Phase 1:

- production `PredictionControl` and `PredictionCollateral` are not implemented
  or wired; live foreign-collateral existence, mirror, freeze, and pause policy
  belongs to Phase 2
- benchmark checks are compile gates only; Nexus production weights must be
  generated in Phase 7
- fuzz crates compile, but campaigns and the remaining Phase 7 fuzz targets have
  not been run
- the upstream Parimutuel `tests/assets.rs` draft remains intentionally
  unregistered, matching the selected upstream snapshot
- production runtime, Runtime API implementations, node RPC, E2E, metadata,
  try-runtime, and rollout work remain later-phase deliverables

Against `docs/ZEITGEIST_FULL_PORT_DEV_SPEC.md`, Phase 1 strictly satisfies its
three exit conditions: SCALE golden tests pass, all 13 pallets pass the valid
WASM `no_std` gate, and all available upstream mock suites run and pass. This is
not a claim that the full-port Definition of Done or Phase 2-8 gates are complete.
