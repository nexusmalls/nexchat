# Upstream provenance / 上游来源

This directory vendors the exact source files published as `pallet-ismp
2512.2.0`. It remains package-name and version compatible with upstream so
existing runtime and pallet consumers require no API changes.

本目录 vendor 了 `pallet-ismp 2512.2.0` 发布包中的原始源文件，并保持与上游一致的
package 名称和版本，因此现有 runtime 与 pallet 调用方无需修改 API。

- Upstream repository / 上游仓库:
  `https://github.com/polytope-labs/hyperbridge`
- Upstream commit / 上游提交:
  `3979482228d9001f0463f3192524fa41bc76989b`
- Upstream path / 上游路径: `modules/pallets/ismp`
- Published crate / 发布 crate: `pallet-ismp 2512.2.0`
- crates.io checksum / crates.io 校验和:
  `fcddc9b37ccb02dd9bfc88c455f98ab4d74b66a727d04c28b72742d5b35fb6bb`
- License / 许可证: Apache-2.0

The commit and source path are recorded by the published crate's
`.cargo_vcs_info.json`; the checksum is recorded by the crates.io entry in the
root `Cargo.lock`.

提交和源码路径来自发布包内的 `.cargo_vcs_info.json`；校验和来自根目录
`Cargo.lock` 中的 crates.io 记录。

## Local patch scope / 本地补丁范围

The only functional dependency change is removal of the unused
`pallet-migrations` item from the runtime `polkadot-sdk` feature list. The
development-only `polkadot-sdk` dependency also disables default features so
selecting this workspace package for a no-std Wasm check does not activate its
test dependency's std closure. Both manifest corrections are mirrored in
`Cargo.toml.orig` so the normalized and original manifests agree.

唯一的功能性依赖改动，是从 runtime `polkadot-sdk` feature 列表移除未使用的
`pallet-migrations`。仅开发使用的 `polkadot-sdk` 依赖同时关闭 default features，
避免直接选择该 workspace package 做 no-std Wasm 检查时激活测试依赖的 std 闭包。
两处 manifest 修正均同步到 `Cargo.toml.orig`，使规范化 manifest 与原始 manifest
保持一致。

Upstream source files do not reference the `pallet-migrations` crate.
`MigrationWeightInfo` is defined internally in `src/weights.rs`. Removing the
feature therefore does not alter pallet APIs, storage layout, migrations,
protocol behavior, or wire format; it only prevents the unrelated
`sp-state-machine` no-std dependency path from being enabled.

上游源码未引用 `pallet-migrations` crate；`MigrationWeightInfo` 在
`src/weights.rs` 内部定义。因此移除该 feature 不改变 pallet API、storage
layout、migration、协议行为或 wire format，只会阻止无关的
`sp-state-machine` no-std 依赖路径被启用。

All Rust source files, the package name, version, features, and remaining
dependencies are unchanged from the published crate. Future upstream updates
must be reviewed as new runtime changes rather than merged automatically.

所有 Rust 源文件、package 名称、版本、features 和其余依赖均与发布包一致。未来上游
更新必须作为新的 runtime 变更单独审查，不得自动合并。
