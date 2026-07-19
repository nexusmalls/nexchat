# Prediction Markets Runtime API

This crate exposes two read-only runtime APIs:

- `PredictionMarketsApi`: deterministic outcome-share identifiers retained from
  the upstream client surface.
- `PredictionViewApi`: Nexus bounded views for market summaries, outcomes,
  deadlines, reports, canonical Neo Swaps prices, redeemable balances, Court
  summaries, collateral mirror state, and global control state.

All `PredictionViewApi` methods are keyed lookups. They do not iterate an
unbounded storage prefix.

本 crate 提供两个只读 runtime API：

- `PredictionMarketsApi`：保留上游客户端使用的确定性 outcome share 标识查询。
- `PredictionViewApi`：Nexus 有界视图，覆盖市场摘要、结果资产、期限、报告、标准
  Neo Swaps 价格、可赎回余额、Court 摘要、抵押镜像状态和全局控制状态。

所有 `PredictionViewApi` 方法均为按 key 查询，不执行无界 storage 前缀遍历。
