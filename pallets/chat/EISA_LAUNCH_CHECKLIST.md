# EISA Launch Blockers / 上线前阻塞项检查表

> **Scope / 范围**：`pallet-chat-sync` + NexChat EISA Standard 档位 + ADR §0 A+B+C 全栈  
> **ADR**：[`CHAT_SYNC_ANCHOR_ADR.md`](./CHAT_SYNC_ANCHOR_ADR.md) v2.2（当前 **Proposed**，待 Accepted）  
> **Verdict / 结论**：链上 pallet **可 devnet 灰度**；EISA Standard **不可立即全量上线**（见下方 P0）

---

## How to use / 用法

- [ ] = open blocker · `- [x]` = done · **Owner** = suggested team  
- **Verify** = objective pass criteria (attach log / screenshot / tx hash in PR)  
- Priority: **P0** = must before mainnet Standard / production EISA · **P1** = before public beta · **P2** = polish

---

## A. Governance & docs / 治理与文档

| ID | P | Item | Owner | Verify |
|----|---|------|-------|--------|
| G-1 | **P0** | ADR v2.2 评审收口，Status → **Accepted**（或记录明确 devnet-only 决议） | Chat + Ops | ADR header updated; link to review notes |
| G-2 | P1 | 更新 ADR §4/§10「待实现」→ 脚本已实现（backup/restore/pinner 等） | Docs | ADR diff; no stale「待实现」on existing scripts |
| G-3 | P1 | `pallets/chat/README.md` 收录 `pallet-chat-sync` + Runtime API / RPC | Chain | README lists sync pallet index & extrinsics |

---

## B. Chain — `pallet-chat-sync` / 链上 C 层

| ID | P | Item | Owner | Verify |
|----|---|------|-------|--------|
| C-1 | ~~P0~~ ✅ | ~~补 `benchmarking.rs` + 跑 runtime benchmarks~~ 已完成：`benchmarking.rs` + dev 链实测权重（steps=50/repeat=20）写入 `weights.rs`；主网前在基准硬件重跑 | Chain | done 2026-06-12 |
| C-2 | **P0** | `AnchorDeposit` 经济评审（当前 `UNIT/2` 占位，ADR §11.5；实测权重已就绪可作输入） | Chain + Econ | Signed-off param in `runtime/src/configs/mod.rs` |
| C-3 | ~~P1~~ ✅ | ~~补命名抢跑单测~~ 已完成：`mempool_front_run_of_first_publish_only_donates_deposit`（修正 §5.5 论证：原封载荷抢跑会落账，仅替持有者垫付押金） | Chain | done 2026-06-12 |
| C-4 | P2 | try-runtime / 集成 smoke：`publish → sync_anchor RPC → clear` on dev runtime | Chain | CI or manual log attached |
| C-5 | ~~P0~~ ✅ | pallet 深审硬化（ADR §11.7 / v2.3）：① 等值重发幂等 no-op（限频骚扰修复）② `ClearedAt` 墓碑防 clear 后历史复活 ③ `ForceOrigin` `force_clear_sync_anchor` 治理逃生门 ④ `storage_version(1)`；配套 5 个新单测 | Chain | done 2026-06-12 |

**Already OK / 已通过**

- [x] ADR §5.4–§5.5 合同：`AnchorId`、锚签名、LWW、`MaxClockSkew`、`MinBlocksBetweenPublish`、depositor 语义  
- [x] **20** pallet tests（18 单测 + 2 mock 完整性；`--features runtime-benchmarks` 另含 bench 测试套件）  
- [x] 跨语言冻结向量：`tests.rs` + `nexchat/src/store/syncAnchor.test.ts`  
- [x] Runtime wiring：`ChatSync` + `ChatSyncApi` + `chat_syncAnchor` RPC  

---

## C. Hard prerequisite §5.0 — `vault_master` / 硬前置换根

| ID | P | Item | Owner | Verify |
|----|---|------|-------|--------|
| V-1 | **P0** | 生产路径禁止 `initForTest(address)` 作为默认根（仅 mock / 显式测试） | NexChat | Code audit; no silent legacy root in prod build |
| V-2 | **P0** | **产品范围声明**：EISA Standard 当前仅支持**桌面 keyring / 可提取 pair secret**；extension 注入器钱包**不在** Standard 覆盖内（无 `vault_master` → 无链锚） | Product + NexChat | Release notes + settings copy |
| V-3 | **P0** | 存量用户自动迁移幂等：`encryptedLocalStore` KDF marker + blob v2 回退（ADR §5.0） | NexChat | Migration test green; spot-check upgraded account |
| V-4 | **P0** | 验收：公开 SS58 **不可**派生 `anchor_id` / `K_sync`（ADR §12） | NexChat | `vaultMaster.test.ts` + manual negative test documented |
| V-5 | P1 | Extension 钱包长期方案（若产品需要 browser Standard）：设计评审 | Product | ADR amendment or separate track |

---

## D. Hard prerequisite §5.8 — blob multi-point survival / 硬前置多点存活

| ID | P | Item | Owner | Verify |
|----|---|------|-------|--------|
| B-1 | **P0** | **热层**：`relay-pinner` 部署到**异机** IPFS（≠ Relay 主机），`PINNER_IPFS_API` 指向 remote cluster | Ops | Hostname / region ≠ relay; pin list in pinner state file |
| B-2 | **P0** | **持久层**：`relay-chain-pinner` 以**运营者账户**跑 Standard pin（非用户账户，ADR 隐私红线） | Ops + Chain | Extrinsic signer = operator; subject id documented |
| B-3 | P1 | **灾备底**：`relay-crust-pinner` + W3Auth PS + CRU 预付池 / 余额告警（ADR §5.8） | Ops | Daily tick log; order ids in state file |
| B-4 | ~~P0~~ ✅ | ~~扩展 `relay-sync-audit.mjs`~~ 已完成：§5.8 三项检查（`IPFS_GATEWAYS` ≥2 网关取回 + pinner 节点 pin 状态 + Crust 订单覆盖/可选 PSA 实时查询）；未配置检查组默认计失败（`AUDIT_ALLOW_SKIP=1` 放宽）；exit code 即验收判据 + 6 单测；生产跑通留样仍归 B-5 演练 | Ops | done 2026-06-12（代码）；B-5 drill 附样例输出 |
| B-5 | **P0** | 断开 Relay 主机 + 同机 IPFS 后，锚内 CID 仍可取回（ADR §12） | QA + Ops | Recorded drill: `ipfs cat` via independent gateway succeeds |
| B-6 | P1 | 客户端 blob 8MB 上限与 pinner 10MB 拒收线对齐（ADR §5.8） | NexChat | Upload cap enforced; oversize logged |

---

## E. Layer B — ops backup / 运维备份

| ID | P | Item | Owner | Verify |
|----|---|------|-------|--------|
| O-1 | **P0** | `relay-backup-to-ipfs.sh` timer 生产化 + `BACKUP_OFFSITE_CMD` 异地 `latest-backup.json` | Ops | Timer active; offsite manifest reachable from second host |
| O-2 | **P0** | Restore 演练：`relay-restore-from-ipfs.sh` → `admin_stats` 指针/spent 与备份前一致（ADR §12） | Ops | Drill write-up with before/after counts |
| O-3 | P1 | GPG 口令异地保管 + 轮换 runbook（ADR §4.2） | Ops | Runbook in ops docs |

---

## F. Client — EISA Standard / 客户端

| ID | P | Item | Owner | Verify |
|----|---|------|-------|--------|
| F-1 | ~~P0~~ P2 | ~~修复 `remoteHasBackup()`~~ 复核降级：两个调用方均经 `offchainSyncEnabled()` 门控（要求 `relayWs`），无 relayWs 分支不可达；可作防御性修复 | NexChat | audited 2026-06-12 |
| F-2 | ~~P0~~ ✅ | ~~§6.5 epoch bump 成功恢复时展示~~ 已完成：`OffchainSyncBanner` 独立 warn 条，`phase === "ok"` 仍显示，可手动知晓关闭 | NexChat | done 2026-06-12 |
| F-3 | ~~P0~~ ✅ | ~~接线 `CoordinatedRestoreResult`~~ 已完成：`appStore.offchainSync` 类型含编排标志 + `dismissEpochBump` 动作 + banner 消费；`needsEpochBump` 不再以写回成功为前提（relay 不可达时不吞提示） | NexChat | done 2026-06-12 |
| F-4 | ~~P0~~ ✅ | ~~§6.5 灾后编排（最小可行）~~ 已完成：① relay 写回（coordinator）② inbox 重注册（`bootstrapDelivery` 在 restore 之后按序执行）③ epoch bump 落为**可执行动作**——`InboxManager.bumpEpoch()`（先持久化再重注册，令牌绑定 epoch 即作废旧令牌）+ `appStore.bumpInboxEpoch` + banner「重置信箱纪元」按钮 ④ banner 附 MLS 重入群/重握手引导文案；3 个新单测 | NexChat | done 2026-06-12 |
| F-5 | ~~P0~~ ✅ | ~~E2E：`coordinator.restore()`~~ 已完成：`offchainSyncCoordinator.restore.test.ts`（6 集成场景：注入/写回/标志/relay 不可达/链读失败/幂等）+ `e2e:sync-restore`（实链全路径，已对 dev 节点跑通）；blob 层（IPFS cat）仍为假实现，§5.8 生产验收另见 B 组 | QA | done 2026-06-12 |
| F-6 | P1 | E2E：B restore 后链较新 manifest 按字段 LWW 覆盖 relay（ADR §12） | QA | Test or drill log |
| F-7 | P1 | §14.6 统一 relay 短 debounce（当前各模块独立 1.5s timer） | NexChat | Single coordinator `markDirty` path only |
| F-8 | P2 | §6.5 MLS 重入群 / 1:1 重握手引导（设计代价，非全量恢复） | Product | Copy in recovery wizard |

**Already OK / 已通过**

- [x] `syncAnchor.ts` + 冻结向量；`offchainSyncCoordinator` hash-skip + 链 debounce + 重试队列  
- [x] 三源逐字段 LWW + 链字段注入 local + relay 写回（逻辑 + 单测）  
- [x] Settings 档位 Standard / Relay-only + 隐私文案（含 Crust CID 披露）  
- [x] P3 burner payer（`VITE_SYNC_ANCHOR_PAYER=burner`）+ live e2e in `syncAnchor.e2e.test.ts`  
- [x] 链锚失败不阻塞发消息（async flush）  

---

## G. Acceptance matrix — ADR §12 snapshot / 验收快照

| ADR §12 item | Status | Blocker IDs |
|--------------|--------|-------------|
| §5.0 vault_master + 迁移 | ⚠️ 代码有，rollout 未证 | V-1…V-4 |
| B restore 一致性 | ⚠️ 脚本有，演练未证 | O-1, O-2 |
| C 助记词 + 空 Relay 恢复 | ✅ 集成 + 实链 E2E（blob 层假实现，§5.8 另计） | B-5 |
| §5.8 blob 多点存活 | ⚠️ audit 工具已闭环（B-4）；生产部署 + 演练未证 | B-1…B-3, B-5 |
| §6.5 灾后重建编排 | ✅ ①②③④ 全部落地（写回 / 重注册按序 / epoch bump 可执行 / MLS 引导文案） | — |
| 授权 / 时钟 / rate limit | ✅ pallet 20 tests + 实测权重（重放骚扰/clear 复活/逃生门已硬化，C-5） | — |
| Relay-only 无链写 | ✅ `chainEnabled()` + tier | — |
| Scale：链失败不阻塞聊天 | ✅ async + retry queue | — |

---

## H. Launch tiers / 分项上线建议

### H.1 Devnet / 内测链 — `pallet-chat-sync` only

**Gate：** C-1, C-2 可 defer 到 mainnet 前；G-1 devnet 决议即可  

- [ ] Runtime 已含 `ChatSync`  
- [ ] `cargo test -p pallet-chat-sync` green  
- [ ] 内测客户端可 `publish_sync_anchor` / `chat_syncAnchor`  

### H.2 Testnet beta — EISA Standard（受控用户）

**Gate：** 全部 **P0** in A–F  

### H.3 Mainnet — EISA Standard 默认开

**Gate：** P0 + P1 + ADR **Accepted** + 至少一次完整 §12 演练记录  

### H.4 Relay-only 档位（无链锚）

**Gate：** O-1, O-2, B-1（热层 pin）；**不依赖** sync pallet  

---

## I. Suggested PR / issue split / 建议拆票

1. **chain/sync-bench-deposit** — C-1, C-2, C-3, G-3  
2. **nexchat/eisa-restore-gates** — F-1, F-2, F-3, F-5  
3. **nexchat/eisa-post-disaster-ux** — F-4, F-8  
4. **ops/eisa-blob-survival** — B-1…B-5, B-4, O-1, O-2  
5. **gov/eisa-adr-accepted** — G-1, G-2  
6. **product/eisa-wallet-scope** — V-2, V-5  

---

## J. One-line summary / 一句话

**代码侧 P0 已清零（benchmark/权重、pallet 硬化、§6.5 编排、§5.8 audit 工具、恢复 E2E 均落地）；在用户侧开 Standard 档之前，剩余阻塞全部在代码之外——§5.0 rollout 证明（V 组）、§5.8 生产 pin 部署 + 断机演练（B-1/B-2/B-5）、Layer B 备份演练（O-1/O-2）、押金经济评审（C-2）与 ADR Accepted（G-1）。**
