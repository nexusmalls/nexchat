# Nexus E2E

这是一个重新整理后的 E2E 测试系统，围绕两层构建：

- 运行时契约检查：在元数据层面对 pallets、calls、storage 和 events 做 ABI 级校验。
- Smoke 场景：一小组带签名的流程测试，用来反映当前运行时的实际行为。

Entity 专用 smoke 使用说明：

- 参见 [`ENTITY_SMOKE_USAGE.md`](./ENTITY_SMOKE_USAGE.md)，了解 5 个基于角色的 Entity smoke 测试套件、适用场景，以及推荐的开发工作流。

## 参与账户

现在 e2e runner 会从 `scripts/e2e/framework/test-accounts-*.json` 加载签名账户，而不是使用写死的开发 seed。

默认行为：

- 会匹配 `WS_URL` 选择的链端点。
- 如果没有设置 `WS_URL`，默认使用 `ws://127.0.0.1:9944`。
- 会自动选择 `scripts/e2e/framework/` 下最新的 `test-accounts-*.json` 文件，且该文件顶层的 `network` 必须与该端点匹配。

覆盖方式：

- 设置 `E2E_ACTORS_FILE` 可强制指定某个账户文件。
- 你可以传绝对路径，也可以只传 `scripts/e2e/framework/` 目录中的文件名。

示例：

- `E2E_ACTORS_FILE=test-accounts-2026-03-20T01-03-22-751Z.json npm run e2e:entity:buyer`
- `WS_URL=wss://202.140.140.202 E2E_ACTORS_FILE=test-accounts-2026-03-20T00-38-47-605Z.json npm run e2e:entity:buyer`

所选 JSON 文件中的角色映射：

- `accounts[0]` -> `alice`（也会作为水龙头账户，在自动补款给其他参与者时使用）
- `accounts[1]` -> `bob`
- `accounts[2]` -> `charlie`
- `accounts[3]` -> `dave`
- `accounts[4]` -> `eve`
- `accounts[5]` -> `ferdie`

重要说明：

- 所选文件中的第一个账户，必须在 `ensureFunds*()` 执行时拥有足够余额，才能为其他账户补款。
- 如果启用 `E2E_TRACE_BOOTSTRAP=1`，runner 会在启动时打印出实际选中的 actor 文件。

入口命令：

- `npm run e2e`
- `npm run e2e:list`
- `npm run e2e:contracts`
- `npm run e2e:smoke`
- `npm run e2e:entity:buyer`
- `npm run e2e:entity:seller`
- `npm run e2e:entity:commission`
- `npm run e2e:entity:governance`
- `npm run e2e:entity:market`
- `npm run e2e:entity:smoke`
- `npm run e2e:remote:list`
- `npm run e2e:remote:contracts`
- `npm run e2e:remote:inspect`
- `npm run e2e:remote:smoke:write`
- `npm run e2e:typecheck`

远程命令默认连接到 `wss://202.140.140.202`，并且会在当前进程中关闭 TLS 证书校验，因为该端点当前使用的是自签名证书。

- `e2e:remote:contracts` 和 `e2e:remote:inspect` 是只读操作。
- `e2e:remote:smoke:write` 会提交交易，并修改远程链状态。

`mytests/` 下也支持直接运行独立运维/审计脚本，例如：

- `SUDO_URI='//Alice' node --import tsx e2e/mytests/validator-set-count.ts --dry-run --expect-validator <SS58>`
- `SUDO_URI='//Alice' node --import tsx e2e/mytests/validator-set-count.ts --count 4 --force-new-era --expect-validator <SS58>`
- `SUDO_URI='//Alice' node --import tsx e2e/mytests/validator-set-count.ts --dry-run --json --expect-validator <SS58>`
- `SUDO_URI='//Alice' node --import tsx e2e/mytests/validator-remove.ts --stash <SS58> --dry-run`
- `SUDO_URI='//Alice' node --import tsx e2e/mytests/validator-remove.ts --stash <SS58> --spans 0 --force-new-era`
