# CLAUDE.md

## 你在这个仓库中的角色

你是 Claude Code，正在 nexus 仓库中协助进行 Substrate / Rust / TypeScript / E2E 相关开发。

这个仓库体量较大、模块较多、跨 pallet 依赖明显。默认做法应当是：先定位边界，再做最小必要修改。

## 开始前先做什么

开始任何非小改动前，优先阅读：
- `README.md`
- 目标模块对应的 `docs/` 文档
- 目标 crate / pallet 的 `lib.rs`、`mock.rs`、`tests.rs` 或相关子模块
- runtime 接线相关文件（如果改动涉及 wiring）：`runtime/src/lib.rs`、`runtime/src/configs/`

不要只凭目录名或猜测动手。

## 仓库地图

- `node/`：节点 CLI、RPC、共识接线
- `runtime/`：Runtime 入口、pallet 注册、configs
- `pallets/`：主要链上业务逻辑
  - `pallets/entity/`：Entity 平台
  - `pallets/trading/`：NEX Market
  - `pallets/dispute/`：托管/证据/仲裁
- `scripts/`：TypeScript E2E 与辅助脚本
- `docs/`：设计、审计、方案文档

## 工作方式

- 永远优先最小必要修改，不做顺手重构。
- 先读再改；不要对未读过的代码提出结构性修改。
- 优先复用现有 trait、types、hooks、providers、bridges、测试模式。
- 不要为了“一次使用”新建 helper / abstraction，除非重复明显存在。
- 不要把一次性调试代码、临时脚本、日志输出长期留在仓库里。
- 修改时注意是否会影响 mock、tests、runtime wiring、runtime api、脚本调用方。

## 针对本仓库的具体提醒

### 改 pallet 时

通常至少检查：
- pallet 入口和子模块
- `Config` trait
- `mock.rs`
- 单测 / 基准测试（如存在）
- 相关 trait/provider 的调用方

如果是 Entity / Commission 相关改动，要特别留意跨模块影响：
- `member`
- `shop`
- `order`
- `token`
- `governance`
- commission plugins

### 改 runtime wiring 时

除了 pallet 本身，还应检查：
- `runtime/src/lib.rs`
- `runtime/src/configs/`
- runtime API / metadata 暴露面
- 外部脚本或前端是否依赖这些接口

### 改 E2E / scripts 时

- 优先沿用 `scripts/e2e/` 现有 flow、runner、assertion 结构。
- 不要重新发明测试框架。
- 先看 `scripts/docs/NEXUS_TEST_PLAN.md`。

## 高优先级提醒

- 以后新增代码中，`pallets/` 下的模块头、公开核心 API、关键 `Config` 和跨模块边界说明必须使用英文 + 中文；触达已有核心公开注释时顺手补双语。

## 常用命令

以 `README.md` 为准，常用命令如下：

```bash
cargo build --release
./target/release/nexus-node --dev
cargo test
cargo test -p pallet-commission-core
cargo test -p pallet-entity-token
cargo test -p pallet-nex-market
cd scripts && npm run e2e
```

## 测试期望

- 小改动：至少运行直接受影响 crate / pallet 的测试。
- 跨 pallet trait、hook、bridge、storage、runtime config 改动：补充运行关联模块测试。
- 改脚本或链上接口联动：优先检查是否已有对应 E2E 场景。
- 如无法运行测试，要明确说明未验证项和原因。

## 优先参考的文档

- `README.md`
- `scripts/docs/NEXUS_TEST_PLAN.md`
- `docs/NEX_MARKET_AUDIT.md`
- `docs/ENTITY_ORDER_TOKENSALE_DISCLOSURE_AUDIT.md`
- `docs/ENTITY_MAINNET_MISSING_FEATURES.md`

如果进入某个子系统工作，优先继续读该子系统文档，而不是重复扫全仓。

## 注释语言规范

- 以后新增代码中，`pallets/` 下的 crate / module 顶部说明（`//!`）必须使用英文 + 中文。
- 以后新增代码中，公开核心 API 的 Rust doc 必须使用英文 + 中文，包括关键 `pub` trait、struct、enum、type、核心 `pub fn`、extrinsic、runtime API，以及关键 `Config` associated types / constants。
- 以后新增代码中，关键业务规则说明、跨模块边界说明（provider / hook / bridge / runtime wiring）必须使用英文 + 中文。
- 修改已有核心公开注释时，若触达该区域，应顺手补成英文 + 中文。
- `tests.rs`、`mock.rs`、`weights.rs`、`benchmarking.rs`、机械性 storage 注释、低价值内部注释不强制双语。
- 默认写法：英文在前，中文在后；两种语言语义必须一致。
- 不要为了满足规范而添加低价值注释；如果一条注释本身没有信息量，就不要把它翻成双语。
- 历史文件不再做全仓式批量补齐；后续按触达模块逐步补齐即可。

## 提交前自检

- 只保留与当前任务直接相关的改动。
- 不提交密钥、`.env`、本地缓存、链数据、测试输出。
- 检查是否误改无关文件。
- 如果改动影响面较大，说明影响范围与验证方式。

## 不确定时的默认策略

先缩小范围，再实现；先读现有模式，再补代码；先跑最相关测试，再考虑扩大验证范围。
