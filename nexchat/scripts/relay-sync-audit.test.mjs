// EN: Unit tests for relay-sync-audit (§5.8 acceptance tool): gateway list parsing,
// Crust state file resolution, and verdict aggregation (FAIL / SKIP semantics).
// CN: relay-sync-audit（§5.8 验收工具）单测：网关列表解析、Crust state 文件定位、
// 结论汇总（FAIL / SKIP 语义）。

import { test } from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import {
  configuredGateways,
  crustStateFile,
  summarizeChecks,
  POINTER_SLOTS,
} from "./relay-sync-audit.mjs";

test("POINTER_SLOTS covers six cloud-sync slots (prod probe parity)", () => {
  assert.equal(POINTER_SLOTS.length, 6);
  const labels = POINTER_SLOTS.map((s) => s.label);
  assert.deepEqual(labels, [
    "contacts",
    "convIndex",
    "msgArchive",
    "mlsVault",
    "handoff",
    "mlsSigning",
  ]);
  for (const slot of POINTER_SLOTS) {
    assert.match(slot.fetch, /_fetch$/);
    assert.match(slot.reply, /_reply$/);
  }
});

test("configuredGateways parses IPFS_GATEWAYS and trims trailing slashes", () => {
  const env = { IPFS_GATEWAYS: "https://gw1/ipfs/, https://gw2/ipfs" };
  assert.deepEqual(configuredGateways(env), ["https://gw1/ipfs", "https://gw2/ipfs"]);
});

test("configuredGateways falls back to single IPFS_GATEWAY then default", () => {
  assert.deepEqual(configuredGateways({ IPFS_GATEWAY: "https://only/ipfs/" }), [
    "https://only/ipfs",
  ]);
  assert.equal(configuredGateways({}).length, 1);
});

test("crustStateFile prefers explicit env over RELAY_DATA_DIR default", () => {
  assert.equal(crustStateFile({ CRUST_PINNER_STATE_FILE: "/x/state.json" }), "/x/state.json");
  assert.equal(
    crustStateFile({ RELAY_DATA_DIR: "/data" }),
    path.join("/data", "relay-crust-pinner-state.json"),
  );
  assert.equal(crustStateFile({}), null);
});

test("summarizeChecks: any FAIL fails the audit", () => {
  const { pass, failures } = summarizeChecks([
    { verdict: "PASS" },
    { verdict: "FAIL", name: "pin" },
  ]);
  assert.equal(pass, false);
  assert.equal(failures.length, 1);
});

test("summarizeChecks: SKIP fails by default (production must run all three checks)", () => {
  const checks = [{ verdict: "PASS" }, { verdict: "SKIP", name: "crust" }];
  assert.equal(summarizeChecks(checks).pass, false);
  assert.equal(summarizeChecks(checks, { allowSkip: true }).pass, true);
});

test("summarizeChecks: all PASS passes", () => {
  assert.equal(summarizeChecks([{ verdict: "PASS" }, { verdict: "PASS" }]).pass, true);
});
