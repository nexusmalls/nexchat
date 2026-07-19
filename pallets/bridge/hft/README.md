# Nexus Hyper Fungible Token pallet

Minimal audited fork of Hyperbridge's official
`pallet-hyper-fungible-token 2512.0.0`.

Hyperbridge 官方 `pallet-hyper-fungible-token 2512.0.0` 的 Nexus 最小审计 fork。

The pallet retains the official `pall_hft` module ID, message ABI, storage
layout and EVM compatibility. Nexus-only changes close partial-commit paths in
outbound dispatch and registry governance. See [UPSTREAM.md](UPSTREAM.md) for
the pinned source and complete patch boundary.

本 pallet 保留官方 `pall_hft` module ID、Message ABI、storage layout 和 EVM
兼容性。Nexus 本地修改仅用于修复出站 dispatch 与 registry 治理的部分提交风险。锁定
来源和完整补丁边界见 [UPSTREAM.md](UPSTREAM.md)。
