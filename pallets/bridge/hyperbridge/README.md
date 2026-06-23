# pallet-hyperbridge (vendored)

Vendored copy of Polytope Labs' `pallet-hyperbridge`, adapted so it compiles
against the only installable `ismp` release.

## Why vendored / 为什么 vendor

The published `ismp 2512.0.0` was **yanked** from crates.io; the only available
release is `ismp 2512.1.0`, which is a **breaking change** within the 2512 line
(it removed `router::Response` / `router::PostResponse` / `router::Timeout` and
`IsmpDispatcher::dispatch_response`). The published `pallet-hyperbridge 2512.0.0`
and `ismp-grandpa 2512.0.0` only target the yanked `ismp 2512.0.0` and **do not
compile** against `ismp 2512.1.0`, and there is no newer release of them. To
unblock Stage 1b we vendor `pallet-hyperbridge` and patch the minimal API delta.

This matches the Nexus D3 = (c) decision (vendor over depending on unpublished /
unstable upstream crates). See `docs/HYPERBRIDGE_INTEGRATION.md` §13.

`ismp 2512.0.0` 已从 crates.io 被 **yank**，唯一可用的是 `ismp 2512.1.0`（2512 线内的
**破坏性变更**：移除 `router::Response`/`PostResponse`/`Timeout` 与
`IsmpDispatcher::dispatch_response`）。已发布的 `pallet-hyperbridge 2512.0.0` /
`ismp-grandpa 2512.0.0` 只对应被 yank 的 `ismp 2512.0.0`，对 `ismp 2512.1.0`
**无法编译**，且无更新发布。为推进 Stage 1b，vendor `pallet-hyperbridge` 并打上最小 API 补丁。
这与 Nexus D3=(c) 决策一致（vendor 优于依赖未发布/不稳定上游 crate）。

## Source / 来源

- Upstream crate: `pallet-hyperbridge` `2512.0.0` (crates.io).
- Upstream repo: `polytope-labs/hyperbridge`.
- Files vendored: `src/lib.rs`, `src/child_trie.rs`.

## Changes vs upstream 2512.0.0 / 相对上游的改动

Limited to the `ismp 2512.0.0 → 2512.1.0` API delta and dependency hygiene:

1. **Removed `IsmpDispatcher::dispatch_response`** — the trait method (and the
   `PostResponse` type) no longer exist in `ismp 2512.1.0`. Response-fee
   collection via this dispatcher is dropped; `dispatch_request` is unchanged.
2. **Removed `on_response` / `on_timeout` overrides** — in `ismp 2512.1.0` the
   `IsmpModule` defaults already return `CannotHandleMessage`, and their argument
   types changed (`Response` → `GetResponse`, `Timeout` → `Request`). The module
   never handled responses/timeouts, so it now relies on the defaults.
3. **Imports** — dropped `router::{PostResponse, Response, Timeout}` (kept
   `PostRequest`).
4. **Dependency hygiene** — depend on individual `frame-support` / `frame-system`
   workspace crates instead of the `polkadot-sdk` umbrella (`polkadot_sdk::`
   paths replaced; `Weight` now from `frame_support::weights::Weight`).

`on_accept` (host-param updates + relayer-fee withdrawals) and the child-trie
payment receipts are unchanged from upstream.

## Re-sync / 重新同步

When upstream publishes a `pallet-hyperbridge` compatible with `ismp 2512.1.0`
(or newer), drop this crate, restore the crates.io dependency, and re-run Stage
1b wiring. Until then, audit this vendored copy as Nexus-owned code.
