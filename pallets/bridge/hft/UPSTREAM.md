# Upstream provenance / 上游来源

This crate is a minimal Nexus security fork of Hyperbridge's
`pallet-hyper-fungible-token`.

本 crate 是 Hyperbridge `pallet-hyper-fungible-token` 的 Nexus 最小安全 fork。

- Upstream repository: `https://github.com/polytope-labs/hyperbridge`
- Upstream commit: `3979482228d9001f0463f3192524fa41bc76989b`
- Published crate: `pallet-hyper-fungible-token 2512.0.0`
- crates.io checksum:
  `47e0d84e3141c28031fe61430e06bb0c9f3bf97293ec107d9f5f8d090252b56e`
- License: Apache-2.0
- Nexus fork version: `2512.0.0-nexus.1`

## Local patch scope / 本地补丁范围

The wire protocol, storage layout, pallet/module IDs, message ABI, precision
conversion and timeout format remain unchanged.

wire protocol、storage layout、pallet/module ID、Message ABI、精度转换和 timeout
格式保持不变。

Local changes are restricted to:

1. Transactional outbound `send`, so asset custody/burn and ISMP dispatch commit
   or roll back together.
2. Complete pre-validation and transactional writes for `register_token` and
   `update_token`.
3. One-to-one validation of forward and reverse contract registries.
4. Regression tests and Nexus-generated weights.

本地修改仅限：

1. 为出站 `send` 增加事务保护，使资产托管/销毁与 ISMP dispatch 同时提交或回滚。
2. `register_token` 和 `update_token` 先完整校验，再事务写入。
3. 校验 contract 正反向 registry 的一一对应关系。
4. 增加回归测试和 Nexus 生成的 weights。

## Runtime benchmark record / Runtime 基准记录

The Nexus weights were generated on 2026-07-11 with Substrate benchmark CLI
53.0.0 against the benchmark-enabled Nexus runtime:

- 50 steps and 20 repeats;
- compiled Wasm execution;
- `max` regression output;
- measured PoV mode;
- bounded callback data: 4096 bytes;
- bounded registry inputs: at most 16 additions and 16 removals per call.

Nexus weights 于 2026-07-11 使用 Substrate benchmark CLI 53.0.0 和启用 benchmark
的 Nexus runtime 生成：

- 50 steps、20 repeats；
- compiled Wasm 执行；
- `max` 回归输出；
- measured PoV 模式；
- callback data 上限为 4096 bytes；
- 单次 registry 调用最多新增 16 条、移除 16 条链配置。

The measured `send` path burns a non-native imported asset, pays a non-zero
native relayer fee and dispatches the full ISMP request with the maximum
callback payload. Registry benchmarks execute the complete governance calls,
including origin verification; only benchmark builds replace production
`EnsureNever` with Root.

实测 `send` 覆盖非原生 imported asset 销毁、非零原生 relayer fee 支付和最大 callback
payload 的完整 ISMP request dispatch。registry benchmark 执行包含 origin 校验的完整
治理调用；仅 benchmark build 会把生产 `EnsureNever` 临时替换为 Root。

Any future upstream sync must be reviewed as a new runtime change. Never merge
upstream `main` automatically.

未来同步上游必须作为新的 runtime 变更单独审查，禁止自动合并上游 `main`。
