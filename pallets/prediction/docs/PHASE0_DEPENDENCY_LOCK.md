# Prediction Phase 0 Dependency Lock Report

Date: 2026-07-12

## Decision

The prediction dependency closure is locked to the Nexus FRAME 45 generation.
The stable2409 ORML fork used by Zeitgeist is not used.

The selected ORML source is the upstream `polkadot-stable2512` branch at
`f389cbdb1d37f4113a3784d72542aa080beb299c`. Its development manifest declares
the same FRAME/SP generation as Nexus:

| Core package | Resolved version |
|---|---:|
| `frame-support` | 45.1.3 |
| `frame-system` | 45.0.0 |
| `sp-runtime` | 45.0.0 |
| `sp-core` | 39.0.0 |
| `sp-io` | 44.0.0 |
| `sp-arithmetic` | 28.0.1 |
| `sp-std` | 14.0.0 |
| `parity-scale-codec` | 3.7.5 |
| `scale-info` | 2.11.6 |
| `staging-xcm` | 21.0.2 |

`orml-currencies`, `orml-tokens`, `orml-traits`, and their required
`orml-utilities` dependency are vendored as a minimal source closure. No ORML
asset registry, unknown-tokens, XCM support, or XTokens pallet is included.

## Math lock

| Dependency | Decision |
|---|---|
| `fixed` | Keep exact `1.15.0` for upstream numerical parity |
| `ark-bn254` | Keep `0.5.0`, `default-features = false`, `curve` only |
| `ark-ff` | Keep `0.5.0`, `default-features = false` |
| `hydra-dx-math` | Vendor Zeitgeist's pinned `v37.0.0` math source at `fcedfa7580cfa9ce4878d799bc4ab4eb917f8d8e`; resolve its SP/codec dependencies from the Nexus workspace |

HydraDX math does not bring FRAME crates. Its no-std source compiles against
Nexus `sp-arithmetic 28.0.1`, `sp-std 14.0.0`, codec 3.7.5, and
`primitive-types 0.13.1`.

## Source imports

- `zeitgeist-primitives` at upstream commit `39ad8d60`.
- `zeitgeist-macros` at upstream commit `39ad8d60`.
- `zrml-market-commons` at upstream commit `39ad8d60`.
- `prediction-phase0-smoke`, a Nexus-only compile and asset proof crate.

The first required FRAME 45 adaptation is recorded separately in `UPSTREAM.md`:
`Asset` and `ScalarPosition` derive `DecodeWithMemTracking`.

## Verification

The following commands pass:

```bash
RUSTFLAGS="--cfg substrate_runtime" cargo check -p hydra-dx-math \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zeitgeist-primitives \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p zrml-market-commons \
  --no-default-features --target wasm32-unknown-unknown
RUSTFLAGS="--cfg substrate_runtime" cargo check -p prediction-phase0-smoke \
  --no-default-features --target wasm32-unknown-unknown
cargo test -p prediction-phase0-smoke
```

The smoke dependency graph contains one version of each FRAME/SP/codec package
listed above. `cargo tree -d` still reports normal ecosystem duplicates such as
proc-macro support and cryptographic helper crates; those are not a second
FRAME/SP type universe.

## Asset proof result

The smoke runtime wires:

```text
Balances <- BasicCurrencyAdapter <- ORML Currencies -> ORML Tokens
pallet-assets USDX (900000) <-> escrow <-> ORML ForeignAsset(900000)
```

Passing tests prove:

- Native deposits change `Balances` and never create ORML native issuance.
- Outcome assets exist only in ORML Tokens.
- USDX escrow balance equals mirrored ORML issuance after deposit and withdrawal.
- A failed escrow release rolls back the preceding mirror burn transactionally.

This is a feasibility proof, not the production `PredictionCollateral` pallet.
Whitelist, freeze, governance, weight, and bridge-timeout behavior remain Phase 2
work.

## Known warnings

- `trie-db 0.30.0` is reported as future-incompatible by Cargo; this originates
  in the current SDK closure and is not introduced as a second SDK version.
- Plain host `cargo check --no-default-features` is not a valid runtime check for
  this SDK combination because it attempts to compile `sp-state-machine`
  without its std-only storage types. The accepted gate uses
  `--cfg substrate_runtime` and the WASM target.
