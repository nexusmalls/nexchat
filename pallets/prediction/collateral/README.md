# Prediction Collateral

Nexus-only adapter that mirrors explicitly approved `pallet-assets` collateral
1:1 into ORML `Asset::ForeignAsset(u64)`.

Nexus 专用适配器，将治理显式批准的 `pallet-assets` 抵押品按 1:1 镜像为 ORML
`Asset::ForeignAsset(u64)`。

## Safety boundary / 安全边界

- Deposits move the real asset into this pallet's independent sovereign account
  before minting the ORML mirror.
- Withdrawals burn the ORML mirror before releasing the escrowed real asset.
- Both paths are transactional and require mirror issuance to equal escrow before
  and after mutation.
- New deposits require `Full` mode, whitelist admission, unpaused state, and the
  runtime-provided asset validator. Withdrawals intentionally remain available
  after mode, whitelist, pause, or validator changes.
- No user mirror-balance copy is stored by this pallet.

- 存入时先把真实资产转入本 pallet 的独立主权账户，再铸造 ORML 镜像。
- 提取时先销毁 ORML 镜像，再释放托管的真实资产。
- 两条路径均使用事务，并要求变更前后镜像总发行量等于托管余额。
- 新存入要求 `Full` 模式、白名单、未暂停以及 runtime 资产验证通过；模式、白名单、
  暂停或验证状态变化后仍允许提取。
- 本 pallet 不保存用户镜像余额副本。

The `AssetValidator` production adapter is intentionally deferred to Phase 6.
USDX protocol/PSM readiness belongs in that adapter. Phase 2 weights are
non-production estimates and must be regenerated in Phase 7.

`AssetValidator` 的生产适配器有意留到 Phase 6；USDX protocol/PSM readiness
应在该适配器中实现。Phase 2 权重为非生产估算值，Phase 7 必须重新生成。
