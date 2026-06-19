#!/usr/bin/env node
// EN: Audit relay sync pointers (contacts / conv-index / msg-archive / mls-vault / handoff /
// mls-signing) for given SS58
// accounts, then run the ADR CHAT_SYNC_ANCHOR §5.8 blob-survival acceptance checks on
// every referenced CID:
//   1. retrieval via >= AUDIT_MIN_GATEWAYS independent IPFS gateways (default 2);
//   2. hot-tier pin status on the remote pinner IPFS node (relay-pinner target);
//   3. Crust storage-order coverage (relay-crust-pinner state, optional live PSA query).
// Exit code 0 = all checks pass. An unconfigured check group counts as FAILURE unless
// AUDIT_ALLOW_SKIP=1 (production acceptance must run all three; dev may relax).
// CN: 查询指定 SS58 账户在 relay 上的同步指针（通讯录 / 会话索引 / 消息归档 / MLS vault /
// handoff / PIN 签名钥备份），并对
// 每个被引用的 CID 执行 ADR CHAT_SYNC_ANCHOR §5.8 多点存活验收三项检查：
//   1. 经 >= AUDIT_MIN_GATEWAYS 个独立 IPFS 网关取回（默认 2）；
//   2. 热层异机 pinner IPFS 节点上的 pin 状态（relay-pinner 目标节点）；
//   3. Crust 存储订单覆盖（relay-crust-pinner state，可选 PSA 实时查询）。
// 退出码 0 = 全部通过。未配置的检查组按失败计，除非 AUDIT_ALLOW_SKIP=1
// （生产验收必须三项齐跑；开发环境可放宽）。
//
// Env / 环境变量:
//   RELAY_WS                 relay websocket (default wss://nexusmall.net/nexchat-relay/)
//   IPFS_GATEWAYS            comma-separated gateway bases (e.g. https://gw1/ipfs,https://gw2/ipfs)
//   IPFS_GATEWAY             single-gateway fallback (legacy)
//   AUDIT_MIN_GATEWAYS       min distinct gateways that must serve each CID (default 2)
//   AUDIT_IPFS_API           pinner IPFS API for pin status (fallback PINNER_IPFS_API)
//   RELAY_DATA_DIR           relay data dir (for the default Crust state file path)
//   CRUST_PINNER_STATE_FILE  crust pinner bookkeeping (default $RELAY_DATA_DIR/relay-crust-pinner-state.json)
//   CRUST_PIN_ENDPOINT       optional W3Auth PSA base for live order status
//   CRUST_PIN_TOKEN          bearer token for the PSA query
//   AUDIT_ALLOW_SKIP         1 = unconfigured check groups don't fail the audit

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import WebSocket from "ws";
import { normalizeAccount } from "./relay-ss58.mjs";

const RELAY = process.env.RELAY_WS ?? "wss://nexusmall.net/nexchat-relay/";
const DEFAULT_GATEWAY = "https://nexusmall.net/nexchat/ipfs-gateway/ipfs";

/** EN: Six cloud-sync pointer slots (parity with prod relay probe). CN: 六个云同步指针槽（与生产探测对齐）。 */
export const POINTER_SLOTS = [
  { label: "contacts", fetch: "contacts_fetch", reply: "contacts_reply" },
  { label: "convIndex", fetch: "index_fetch", reply: "index_reply" },
  { label: "msgArchive", fetch: "msg_archive_fetch", reply: "msg_archive_reply" },
  { label: "mlsVault", fetch: "mls_vault_fetch", reply: "mls_vault_reply" },
  { label: "handoff", fetch: "handoff_fetch", reply: "handoff_reply" },
  { label: "mlsSigning", fetch: "mls_signing_fetch", reply: "mls_signing_reply" },
];

function wsFetch(type, replyType, account) {
  return new Promise((resolve) => {
    const ws = new WebSocket(RELAY);
    const request_id = `audit-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    const t = setTimeout(() => {
      ws.close();
      resolve({ error: "timeout" });
    }, 8000);
    ws.on("open", () => {
      ws.send(JSON.stringify({ type, account, request_id }));
    });
    ws.on("message", (raw) => {
      try {
        const m = JSON.parse(String(raw));
        if (m.type === replyType && m.request_id === request_id) {
          clearTimeout(t);
          ws.close();
          resolve(m);
        }
      } catch {
        /* ignore */
      }
    });
    ws.on("error", (e) => {
      clearTimeout(t);
      resolve({ error: String(e.message ?? e) });
    });
  });
}

async function auditAccount(account) {
  const canonical = normalizeAccount(account);
  const replies = await Promise.all(
    POINTER_SLOTS.map((s) => wsFetch(s.fetch, s.reply, canonical)),
  );
  const slot = (m) =>
    m.error ? m : m.cid ? { cid: m.cid, updated_at: m.updated_at } : null;
  const row = { account: canonical };
  for (let i = 0; i < POINTER_SLOTS.length; i++) {
    row[POINTER_SLOTS[i].label] = slot(replies[i]);
  }
  return row;
}

// ---------------------------------------------------------------------------
// §5.8 check 1 — multi-gateway retrieval / 多网关取回
// ---------------------------------------------------------------------------

export function configuredGateways(env = process.env) {
  const multi = (env.IPFS_GATEWAYS ?? "")
    .split(",")
    .map((s) => s.trim().replace(/\/$/, ""))
    .filter(Boolean);
  if (multi.length > 0) return multi;
  const single = (env.IPFS_GATEWAY ?? DEFAULT_GATEWAY).replace(/\/$/, "");
  return [single];
}

async function probeGateway(base, cid) {
  const url = `${base}/${cid}`;
  try {
    const res = await fetch(url, { method: "HEAD", signal: AbortSignal.timeout(8000) });
    return { ok: res.ok, status: res.status, url };
  } catch (e) {
    return { ok: false, error: String(e.message ?? e), url };
  }
}

async function checkGateways(cid, gateways, minGateways) {
  const probes = await Promise.all(gateways.map((g) => probeGateway(g, cid)));
  const okCount = probes.filter((p) => p.ok).length;
  return {
    name: "gateways",
    // EN: a single configured gateway can never satisfy a >=2 requirement — that is a
    // configuration failure, not a SKIP. CN: 只配一个网关永远满足不了 >=2 的要求——
    // 这是配置失败，不是 SKIP。
    verdict: okCount >= minGateways ? "PASS" : "FAIL",
    detail: `${okCount}/${gateways.length} gateways serve ${cid} (need ${minGateways})`,
    probes,
  };
}

// ---------------------------------------------------------------------------
// §5.8 check 2 — hot-tier pin status / 热层 pin 状态
// ---------------------------------------------------------------------------

async function checkPinStatus(cid, ipfsApi) {
  if (!ipfsApi) {
    return {
      name: "pin",
      verdict: "SKIP",
      detail: "AUDIT_IPFS_API / PINNER_IPFS_API not set",
    };
  }
  const url = `${ipfsApi.replace(/\/$/, "")}/api/v0/pin/ls?arg=${cid}`;
  try {
    const res = await fetch(url, { method: "POST", signal: AbortSignal.timeout(15000) });
    const text = await res.text();
    if (!res.ok) {
      // EN: kubo returns 500 "not pinned" for unpinned CIDs. CN: 未 pin 时 kubo 返回 500。
      return { name: "pin", verdict: "FAIL", detail: `not pinned (${res.status}: ${text.slice(0, 120)})` };
    }
    const body = text ? JSON.parse(text) : {};
    const keys = Object.keys(body?.Keys ?? {});
    return keys.length > 0
      ? { name: "pin", verdict: "PASS", detail: `pinned as ${body.Keys[keys[0]].Type}` }
      : { name: "pin", verdict: "FAIL", detail: "no pin entry in response" };
  } catch (e) {
    return { name: "pin", verdict: "FAIL", detail: `pin query failed: ${String(e.message ?? e)}` };
  }
}

// ---------------------------------------------------------------------------
// §5.8 check 3 — Crust order coverage / Crust 订单覆盖
// ---------------------------------------------------------------------------

export function crustStateFile(env = process.env) {
  if (env.CRUST_PINNER_STATE_FILE) return env.CRUST_PINNER_STATE_FILE;
  if (env.RELAY_DATA_DIR) return path.join(env.RELAY_DATA_DIR, "relay-crust-pinner-state.json");
  return null;
}

function readCrustState(file) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return null;
  }
}

async function liveCrustStatus(requestId, env = process.env) {
  const endpoint = (env.CRUST_PIN_ENDPOINT ?? "").replace(/\/$/, "");
  const token = env.CRUST_PIN_TOKEN ?? "";
  if (!endpoint || !token || !requestId || requestId === "ok") return null;
  try {
    const res = await fetch(`${endpoint}/pins/${requestId}`, {
      headers: { authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(15000),
    });
    if (!res.ok) return { ok: false, detail: `PSA ${res.status}` };
    const body = await res.json();
    const status = body?.status ?? "unknown";
    // PSA statuses: queued | pinning | pinned | failed.
    return { ok: status !== "failed", detail: `PSA status=${status}` };
  } catch (e) {
    return { ok: false, detail: `PSA query failed: ${String(e.message ?? e)}` };
  }
}

async function checkCrustOrder(cid, stateFile, env = process.env) {
  if (!stateFile) {
    return {
      name: "crust",
      verdict: "SKIP",
      detail: "CRUST_PINNER_STATE_FILE / RELAY_DATA_DIR not set",
    };
  }
  const state = readCrustState(stateFile);
  if (!state) {
    return { name: "crust", verdict: "FAIL", detail: `state file unreadable: ${stateFile}` };
  }
  const entry = state?.requested?.[cid];
  if (!entry) {
    return { name: "crust", verdict: "FAIL", detail: "no storage order recorded for cid" };
  }
  const live = await liveCrustStatus(entry.requestId, env);
  if (live && !live.ok) {
    return { name: "crust", verdict: "FAIL", detail: `order ${entry.requestId}: ${live.detail}` };
  }
  return {
    name: "crust",
    verdict: "PASS",
    detail: `order ${entry.requestId ?? "?"} at ${new Date(entry.at ?? 0).toISOString()}${live ? ` (${live.detail})` : ""}`,
  };
}

// ---------------------------------------------------------------------------
// Aggregation / 汇总
// ---------------------------------------------------------------------------

/// EN: Pure verdict aggregation — FAIL always fails; SKIP fails unless allowSkip.
/// CN: 纯汇总逻辑——FAIL 必失败；SKIP 仅在 allowSkip 时放行。
export function summarizeChecks(checks, { allowSkip = false } = {}) {
  const failures = checks.filter(
    (c) => c.verdict === "FAIL" || (c.verdict === "SKIP" && !allowSkip),
  );
  return { pass: failures.length === 0, failures };
}

async function main() {
  const accounts = process.argv.slice(2).map((a) => normalizeAccount(a));
  if (accounts.length === 0) {
    console.error("Usage: node relay-sync-audit.mjs <ss58-account> [...]");
    process.exit(1);
  }

  const gateways = configuredGateways();
  const minGateways = Number(process.env.AUDIT_MIN_GATEWAYS ?? 2);
  const ipfsApi = process.env.AUDIT_IPFS_API ?? process.env.PINNER_IPFS_API ?? "";
  const stateFile = crustStateFile();
  const allowSkip = process.env.AUDIT_ALLOW_SKIP === "1";

  console.log(`gateways (${gateways.length}, need ${minGateways}):`, gateways.join(", "));
  console.log("pin api:", ipfsApi || "(unset — pin check will SKIP)");
  console.log("crust state:", stateFile ?? "(unset — crust check will SKIP)");

  const allChecks = [];
  for (const account of accounts) {
    console.log(`\n=== ${account} ===`);
    const row = await auditAccount(account);
    for (const { label } of POINTER_SLOTS) {
      const ptr = row[label];
      console.log(`${label}:`, ptr ?? "NONE");
      if (!ptr || !ptr.cid) continue;
      const checks = [
        await checkGateways(ptr.cid, gateways, minGateways),
        await checkPinStatus(ptr.cid, ipfsApi),
        await checkCrustOrder(ptr.cid, stateFile),
      ];
      for (const c of checks) {
        console.log(`  [${c.verdict}] ${label} ${c.name}: ${c.detail}`);
        allChecks.push({ ...c, account, label, cid: ptr.cid });
      }
    }
  }

  const { pass, failures } = summarizeChecks(allChecks, { allowSkip });
  console.log(
    `\n§5.8 audit: ${allChecks.length} checks, ${failures.length} failing${allowSkip ? " (SKIP allowed)" : ""} → ${pass ? "PASS" : "FAIL"}`,
  );
  if (!pass) {
    for (const f of failures) {
      console.log(`  FAIL ${f.account} ${f.label} ${f.name}: ${f.detail}`);
    }
  }
  process.exit(pass ? 0 : 1);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
