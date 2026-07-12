# Prediction Phase 0 Runtime Budget Baseline

Date: 2026-07-12

## Baseline identity

- Nexus commit: `d14dd20b6efc1898801d9c632be27923f88d2087`
- Working branch: `feat/hyperbridge-integration`
- Runtime spec version observed in source: 104
- Measurement state: dirty working tree containing pre-existing Hyperbridge/USDX
  integration work; prediction pallets are not wired into the runtime.

This baseline must therefore be compared against the same integration branch or
re-measured after those pre-existing changes are committed. It is not a clean
`main` benchmark.

## Runtime artifacts

Produced by:

```bash
cargo build --release -p nexus-runtime
```

| Artifact | Bytes |
|---|---:|
| `nexus_runtime.compact.compressed.wasm` | 2,690,196 |
| `nexus_runtime.compact.wasm` | 19,512,992 |
| `nexus_runtime.wasm` | 20,588,308 |

Compressed runtime SHA-256:

```text
911c2d2fb421c937ad61436f1838754fd8165a8e22bb5b624ea321937daa417a
```

Observed warm release rebuild time was approximately 4 minutes 26 seconds.
This is a developer-machine reference, not a reproducible clean-CI timing.

## Metadata

Runtime metadata was generated natively inside `sp_io::TestExternalities` from
`Runtime::metadata()`:

```text
787,576 bytes
```

The temporary measurement example was removed after use.

## Block limits

Current runtime configuration:

| Budget | Value |
|---|---:|
| Maximum compute weight | 2 seconds of `WEIGHT_REF_TIME_PER_SECOND` |
| Normal dispatch weight ratio | 75% |
| Maximum block length | 8 MiB |
| Normal dispatch length ratio | 85% |
| Block time | 6 seconds |

Prediction integration must report deltas against the artifact and metadata
numbers above. Automatic close/resolve hooks must remain within the initialization
budget derived from these limits.

## Existing branch blocker

Building the complete node currently fails before compilation because
`node/Cargo.toml` requests the `nexus-runtime/usdx-phase1-activation` feature,
while the current `runtime/Cargo.toml` does not define it:

```text
package `nexus-node` depends on `nexus-runtime` with feature
`usdx-phase1-activation` but `nexus-runtime` does not have that feature
```

This mismatch existed in the active non-prediction integration work and is not
fixed by Phase 0. Direct `nexus-runtime` release build succeeds. A node startup,
block-import timing, and RPC metadata cross-check must be repeated after the
USDX feature mismatch is resolved by its owning change.

## Deferred measurements

The following require a runnable node and a controlled benchmark environment:

- empty-block import time;
- saturated-block execution/import time;
- representative storage proof sizes;
- clean release build time and peak memory.

They are runtime-integration performance gates, not evidence against the
dependency or asset feasibility proven in Phase 0.
