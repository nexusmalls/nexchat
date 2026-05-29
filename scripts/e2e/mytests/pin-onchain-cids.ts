#!/usr/bin/env tsx
/**
 * Scan on-chain storage for IPFS CIDs, write them to JSON, and optionally pin active registry CIDs.
 * 扫描链上 storage 中的 IPFS CID，写入 JSON，并可选 pin 仍活跃的 registry CID。
 *
 * One-command loop (query every 5m + pin), after setting PIN_ENDPOINT in e2e/mytests/.env:
 *   cd scripts && node --import tsx e2e/mytests/pin-onchain-cids.ts
 *
 * Or inline:
 *   cd scripts && PIN_ENDPOINT=https://ipfs.example/api/v0/pin/add node --import tsx e2e/mytests/pin-onchain-cids.ts
 *
 * Environment:
 *   WS_URL              — Nexus RPC endpoint, default wss://rpc.nexusmall.net
 *   PIN_ENDPOINT        — Required for pin mode (omit with --dry-run)
 *   PIN_TOKEN           — Optional bearer token (prefer over --pin-token)
 *   PIN_INTERVAL_MINUTES — loop interval, default 5 (query then pin each cycle)
 *   PIN_INTERVAL_HOURS  — legacy hour-based interval (overrides minutes if set)
 *   PIN_TIMEOUT_MS      — fetch timeout, default 30000
 *   PIN_RETRIES         — retry count after first attempt, default 2
 *   PIN_CONCURRENCY     — concurrent pin requests, default 4
 *   SCAN_CONCURRENCY    — parallel storage queries in fast/all scan, default 8
 *   WS_CONNECT_TIMEOUT_MS — RPC WebSocket connect timeout, default 60000
 *   CID_OUTPUT_FILE     — JSON output path, default scripts/all-cids.json
 *   INSECURE_TLS=1      — opt-in: disable TLS certificate verification (dev/self-signed only)
 *   CID_SCAN_PALLETS    — comma-separated pallet filter; overrides default query whitelist
 *   FAIL_ON_PIN_ERRORS  — set to 0 to keep exit 0 when pin failures occur (default 1 in pin mode)
 *   ALERT_WEBHOOK_URL   — optional POST target when pin failures occur (JSON body)
 */

import { rename, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { ApiPromise } from '@polkadot/api';
import dotenv from 'dotenv';

import { connectApi, disconnectApi, captureChainSnapshot } from '../framework/api.js';
import { codecToJson, decodeTextValue } from '../framework/codec.js';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
dotenv.config({ path: path.resolve(SCRIPT_DIR, '.env') });

process.env.WS_URL ??= 'wss://rpc.nexusmall.net';
if (process.env.INSECURE_TLS === '1') {
  process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';
}

type ScanMode = 'registry' | 'fast' | 'all' | 'query-pin';

interface StorageScanTarget {
  pallet: string;
  storage: string;
}

interface CliOptions {
  once: boolean;
  dryRun: boolean;
  json: boolean;
  scanMode: ScanMode;
  palletFilter?: Set<string>;
  intervalMs: number;
  pinEndpoint?: string;
  pinToken?: string;
  pinTimeoutMs: number;
  pinRetries: number;
  pinConcurrency: number;
  scanConcurrency: number;
  wsConnectTimeoutMs: number;
  outputFile: string;
  queryOutputFile: string;
  fullScan: boolean;
  failOnError: boolean;
}

interface CidRecord {
  cid: string;
  sources: string[];
}

interface PinResult {
  cid: string;
  ok: boolean;
  status?: number;
  error?: string;
  attempts?: number;
}

interface CycleSummary {
  startedAt: string;
  finishedAt: string;
  wsUrl?: string;
  dryRun: boolean;
  scanMode: ScanMode;
  fullScan: boolean;
  palletFilter?: string[];
  failOnError: boolean;
  outputFile: string;
  queryOutputFile?: string;
  cidCount: number;
  pinned: number;
  failed: number;
  queryCidCount?: number;
  cids: Array<CidRecord & PinResult>;
}

const CID_V0_PATTERN = /^Qm[1-9A-HJ-NP-Za-km-z]{44}$/;
const CID_V1_PATTERN = /^(baf[1-9A-HJ-NP-Za-km-z]+|k51[1-9A-HJ-NP-Za-km-z]+)$/;
const DEFAULT_OUTPUT_FILE = fileURLToPath(new URL('../../all-cids.json', import.meta.url));
const DEFAULT_QUERY_OUTPUT_FILE = fileURLToPath(new URL('../../all-cids-query.json', import.meta.url));
const DEFAULT_INTERVAL_MS = 5 * 60_000;
/** Known on-chain storages that embed IPFS CID fields. */
const FAST_SCAN_TARGETS: readonly StorageScanTarget[] = [
  { pallet: 'storageService', storage: 'cidRegistry' },
  { pallet: 'entityRegistry', storage: 'entities' },
  { pallet: 'entityShop', storage: 'shops' },
  { pallet: 'entityProduct', storage: 'products' },
  { pallet: 'entityTransaction', storage: 'orders' },
  { pallet: 'entityReview', storage: 'reviews' },
  { pallet: 'entityReview', storage: 'reviewReplies' },
  { pallet: 'entityDisclosure', storage: 'disclosures' },
  { pallet: 'entityDisclosure', storage: 'announcements' },
  { pallet: 'entityDisclosure', storage: 'draftRevisions' },
  { pallet: 'entityKyc', storage: 'kycRecords' },
  { pallet: 'entityKyc', storage: 'upgradeRequests' },
  { pallet: 'evidence', storage: 'evidences' },
] as const;
/** Default pallets when --scan-mode all without --full-scan. */
const DEFAULT_QUERY_PALLETS = [
  'storageService',
  'entityRegistry',
  'entityShop',
  'entityProduct',
  'entityTransaction',
  'entityReview',
  'entityDisclosure',
  'entityKyc',
  'evidence',
] as const;

function parsePositiveInt(raw: string | undefined, fallback: number, label: string): number {
  const value = Number(raw ?? fallback);
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`${label} must be a positive number`);
  }
  return Math.floor(value);
}

function parseBooleanEnv(raw: string | undefined, fallback: boolean): boolean {
  if (raw == null || raw.trim() === '') {
    return fallback;
  }
  const normalized = raw.trim().toLowerCase();
  if (['1', 'true', 'yes', 'on'].includes(normalized)) {
    return true;
  }
  if (['0', 'false', 'no', 'off'].includes(normalized)) {
    return false;
  }
  throw new Error(`Invalid boolean env value: ${raw}`);
}

function resolveDefaultPalletFilter(
  scanMode: ScanMode,
  fullScan: boolean,
  explicitFilter?: Set<string>,
): Set<string> | undefined {
  if (explicitFilter) {
    return explicitFilter;
  }
  if (scanMode !== 'all' || fullScan) {
    return undefined;
  }
  return new Set(DEFAULT_QUERY_PALLETS);
}

function parseIntervalMs(argv: {
  intervalMinutes?: number;
  intervalHours?: number;
}): number {
  if (argv.intervalMinutes != null) {
    return argv.intervalMinutes * 60_000;
  }
  if (argv.intervalHours != null) {
    return argv.intervalHours * 60 * 60_000;
  }
  if (process.env.PIN_INTERVAL_HOURS?.trim()) {
    return parsePositiveInt(process.env.PIN_INTERVAL_HOURS, 1, 'PIN_INTERVAL_HOURS') * 60 * 60_000;
  }
  return parsePositiveInt(process.env.PIN_INTERVAL_MINUTES, 5, 'PIN_INTERVAL_MINUTES') * 60_000;
}

function formatInterval(ms: number): string {
  if (ms % (60 * 60_000) === 0) {
    return `${ms / (60 * 60_000)}h`;
  }
  return `${Math.round(ms / 60_000)}m`;
}
function parsePalletFilter(raw: string | undefined): Set<string> | undefined {
  if (!raw?.trim()) {
    return undefined;
  }
  const items = raw.split(',').map((item) => item.trim()).filter(Boolean);
  return items.length > 0 ? new Set(items) : undefined;
}

function parseCli(argv: string[]): CliOptions {
  let once = false;
  let dryRun = false;
  let json = false;
  let scanMode: ScanMode | undefined;
  let explicitPalletFilter = parsePalletFilter(process.env.CID_SCAN_PALLETS);
  let fullScan = false;
  let failOnError: boolean | undefined;
  let intervalMinutes: number | undefined;
  let intervalHours: number | undefined;
  let pinEndpoint = process.env.PIN_ENDPOINT?.trim() || undefined;
  let pinToken = process.env.PIN_TOKEN?.trim() || undefined;
  let pinTimeoutMs = parsePositiveInt(process.env.PIN_TIMEOUT_MS, 30_000, 'PIN_TIMEOUT_MS');
  let pinRetries = parsePositiveInt(process.env.PIN_RETRIES, 2, 'PIN_RETRIES');
  let pinConcurrency = parsePositiveInt(process.env.PIN_CONCURRENCY, 4, 'PIN_CONCURRENCY');
  let scanConcurrency = parsePositiveInt(process.env.SCAN_CONCURRENCY, 8, 'SCAN_CONCURRENCY');
  let wsConnectTimeoutMs = parsePositiveInt(process.env.WS_CONNECT_TIMEOUT_MS, 60_000, 'WS_CONNECT_TIMEOUT_MS');
  let outputFile = process.env.CID_OUTPUT_FILE ?? DEFAULT_OUTPUT_FILE;
  let queryOutputFile = process.env.CID_QUERY_OUTPUT_FILE ?? DEFAULT_QUERY_OUTPUT_FILE;

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--once') {
      once = true;
    } else if (arg === '--dry-run') {
      dryRun = true;
    } else if (arg === '--json') {
      json = true;
    } else if (arg === '--fast' || arg === '--fast-scan') {
      scanMode = 'fast';
    } else if (arg === '--query-and-pin' || arg === '--query-pin') {
      scanMode = 'query-pin';
    } else if (arg === '--scan-all') {
      scanMode = 'all';
    } else if (arg === '--scan-mode' && argv[i + 1]) {
      const mode = argv[++i];
      if (mode !== 'registry' && mode !== 'all' && mode !== 'fast' && mode !== 'query-pin') {
        throw new Error('--scan-mode must be "registry", "fast", "all", or "query-pin"');
      }
      scanMode = mode;
    } else if (arg === '--pallets' && argv[i + 1]) {
      explicitPalletFilter = parsePalletFilter(argv[++i]);
    } else if (arg === '--full-scan') {
      fullScan = true;
    } else if (arg === '--fail-on-error') {
      failOnError = true;
    } else if (arg === '--no-fail-on-error') {
      failOnError = false;
    } else if (arg === '--interval-minutes' && argv[i + 1]) {
      intervalMinutes = parsePositiveInt(argv[++i], 5, '--interval-minutes');
    } else if (arg === '--interval-hours' && argv[i + 1]) {
      intervalHours = parsePositiveInt(argv[++i], 1, '--interval-hours');
    } else if (arg === '--query-output' && argv[i + 1]) {
      queryOutputFile = argv[++i];
    } else if (arg === '--pin-token' && argv[i + 1]) {
      pinToken = argv[++i];
    } else if (arg === '--pin-timeout-ms' && argv[i + 1]) {
      pinTimeoutMs = parsePositiveInt(argv[++i], 30_000, '--pin-timeout-ms');
    } else if (arg === '--pin-retries' && argv[i + 1]) {
      pinRetries = parsePositiveInt(argv[++i], 2, '--pin-retries');
    } else if (arg === '--pin-concurrency' && argv[i + 1]) {
      pinConcurrency = parsePositiveInt(argv[++i], 4, '--pin-concurrency');
    } else if (arg === '--scan-concurrency' && argv[i + 1]) {
      scanConcurrency = parsePositiveInt(argv[++i], 8, '--scan-concurrency');
    } else if (arg === '--output' && argv[i + 1]) {
      outputFile = argv[++i];
    } else if (arg === '--help' || arg === '-h') {
      printHelp();
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  const resolvedScanMode: ScanMode = scanMode ?? (fullScan ? 'all' : dryRun ? 'fast' : 'query-pin');
  const needsPinEndpoint = !dryRun && (resolvedScanMode === 'registry' || resolvedScanMode === 'query-pin');
  if (needsPinEndpoint && !pinEndpoint) {
    throw new Error('PIN_ENDPOINT or --pin-endpoint is required for pin / query-pin mode');
  }

  const palletFilter = resolvedScanMode === 'all'
    ? resolveDefaultPalletFilter(resolvedScanMode, fullScan, explicitPalletFilter)
    : undefined;
  const resolvedFailOnError = failOnError ?? parseBooleanEnv(process.env.FAIL_ON_PIN_ERRORS, !dryRun);
  const intervalMs = parseIntervalMs({ intervalMinutes, intervalHours });

  return {
    once,
    dryRun,
    json,
    scanMode: resolvedScanMode,
    palletFilter,
    intervalMs,
    pinEndpoint,
    pinToken,
    pinTimeoutMs,
    pinRetries,
    pinConcurrency,
    scanConcurrency,
    wsConnectTimeoutMs,
    outputFile,
    queryOutputFile,
    fullScan,
    failOnError: resolvedFailOnError,
  };
}

function printHelp(): void {
  console.log(`Usage: node --import tsx e2e/mytests/pin-onchain-cids.ts [options]

Options:
  --once                     Run one scan/pin cycle and exit
  --dry-run                  Scan and write JSON, but do not call the pin endpoint
  --json                     Print machine-readable JSON summary
  --scan-mode <registry|fast|all|query-pin>
                             query-pin = fast query then pin registry (default loop)
                             registry = active cidRegistry only
                             fast = known CID storages only (dry-run default)
                             all = every storage item in selected pallets
  --query-and-pin, --query-pin
                             Alias for --scan-mode query-pin
  --fast, --fast-scan        Alias for --scan-mode fast
  --scan-all                 Alias for --scan-mode all
  --full-scan                Scan every pallet/storage (slowest)
  --pallets <a,b,c>          Limit --scan-mode all to selected pallets
  --scan-concurrency <n>     Parallel storage queries, default 8
  --fail-on-error            Exit 1 when pin failures occur (pin mode default)
  --no-fail-on-error         Keep exit 0 even if pin failures occur
  --interval-minutes <n>    Loop interval in minutes, default 5
  --interval-hours <n>       Loop interval in hours (overrides minutes)
  --query-output <path>      Query JSON path for query-pin mode
  --pin-endpoint <url>       Pin endpoint (required for query-pin / registry)
  --pin-token <token>        Bearer token; prefer PIN_TOKEN env var
  --pin-timeout-ms <n>       Pin fetch timeout, default 30000
  --pin-retries <n>          Extra retries after first attempt, default 2
  --pin-concurrency <n>      Concurrent pin requests, default 4
  --output <path>            JSON output path, default ${DEFAULT_OUTPUT_FILE}

Environment:
  PIN_ENDPOINT, PIN_TOKEN, PIN_INTERVAL_MINUTES, PIN_INTERVAL_HOURS
  PIN_TIMEOUT_MS, PIN_RETRIES, PIN_CONCURRENCY, SCAN_CONCURRENCY
  CID_SCAN_PALLETS, CID_OUTPUT_FILE, CID_QUERY_OUTPUT_FILE, INSECURE_TLS=1
  FAIL_ON_PIN_ERRORS, ALERT_WEBHOOK_URL

Default fast-scan storages (--scan-mode fast):
  ${FAST_SCAN_TARGETS.map((item) => `${item.pallet}.${item.storage}`).join(', ')}

Default query pallets (--scan-mode all, unless --full-scan):
  ${DEFAULT_QUERY_PALLETS.join(', ')}
`);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function isLikelyIpfsCid(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed || /\s/.test(trimmed) || /[^\x20-\x7E]/.test(trimmed)) {
    return false;
  }
  return CID_V0_PATTERN.test(trimmed) || CID_V1_PATTERN.test(trimmed);
}

function decodeCidValue(value: unknown): string | undefined {
  const text = decodeTextValue(value)?.trim();
  if (!text || !isLikelyIpfsCid(text)) {
    return undefined;
  }
  return text;
}

function pushCid(records: Map<string, CidRecord>, cid: string, source: string): void {
  const existing = records.get(cid);
  if (existing) {
    if (!existing.sources.includes(source)) {
      existing.sources.push(source);
    }
    return;
  }
  records.set(cid, { cid, sources: [source] });
}

function collectCidFields(value: unknown, source: string, records: Map<string, CidRecord>): void {
  if (value == null) {
    return;
  }

  const decoded = decodeCidValue(value);
  if (decoded) {
    pushCid(records, decoded, source);
    return;
  }

  if (Array.isArray(value)) {
    for (const [index, item] of value.entries()) {
      collectCidFields(item, `${source}[${index}]`, records);
    }
    return;
  }

  if (typeof value !== 'object') {
    return;
  }

  for (const [key, nested] of Object.entries(value as Record<string, unknown>)) {
    collectCidFields(nested, `${source}.${key}`, records);
  }
}

function storageEntrySource(pallet: string, storage: string, key: unknown): string {
  const args = (key as { args?: unknown[] })?.args;
  if (!Array.isArray(args) || args.length === 0) {
    return `${pallet}.${storage}`;
  }
  const renderedArgs = args.map((arg) => (arg as { toString?: () => string })?.toString?.() ?? String(arg)).join(',');
  return `${pallet}.${storage}(${renderedArgs})`;
}

function shouldScanPallet(pallet: string, palletFilter?: Set<string>): boolean {
  return !palletFilter || palletFilter.has(pallet);
}

async function collectActiveRegistryCids(
  api: ApiPromise,
  scanConcurrency: number,
): Promise<CidRecord[]> {
  const records = new Map<string, CidRecord>();
  const storageService = (api.query as Record<string, any>).storageService;
  const cidRegistry = storageService?.cidRegistry;
  const pinMeta = storageService?.pinMeta;

  if (!cidRegistry?.entries || !pinMeta) {
    throw new Error('storageService.cidRegistry or storageService.pinMeta is unavailable on this chain');
  }

  const entries = await cidRegistry.entries() as Array<[unknown, unknown]>;
  await mapConcurrent(entries, scanConcurrency, async ([key, value]) => {
    const cidHashArg = (key as { args?: unknown[] })?.args?.[0];
    const pinMetaValue = await pinMeta(cidHashArg);
    if (pinMetaValue?.isNone) {
      return;
    }

    const cidHash = cidHashArg?.toString?.() ?? String(key);
    const cid = decodeCidValue(codecToJson(value));
    if (!cid) {
      return;
    }
    pushCid(records, cid, `storageService.cidRegistry(${cidHash})`);
  });

  return [...records.values()].sort((a, b) => a.cid.localeCompare(b.cid));
}

async function scanStorageEntries(
  api: ApiPromise,
  target: StorageScanTarget,
  records: Map<string, CidRecord>,
  json: boolean,
): Promise<number> {
  const query = (api.query as Record<string, any>)?.[target.pallet]?.[target.storage];
  if (!query || typeof query.entries !== 'function') {
    console.warn(`[scan-warning] missing ${target.pallet}.${target.storage}`);
    return 0;
  }

  try {
    const entries = await query.entries() as Array<[unknown, unknown]>;
    if (!json) {
      console.log(`[scan] ${target.pallet}.${target.storage} entries=${entries.length}`);
    }
    for (const [key, value] of entries) {
      collectCidFields(
        codecToJson(value),
        storageEntrySource(target.pallet, target.storage, key),
        records,
      );
    }
    return entries.length;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.warn(`[scan-warning] skipped ${target.pallet}.${target.storage}: ${message}`);
    return 0;
  }
}

async function collectFastOnChainCids(
  api: ApiPromise,
  options: Pick<CliOptions, 'json' | 'scanConcurrency'>,
): Promise<CidRecord[]> {
  const records = new Map<string, CidRecord>();
  let totalEntries = 0;

  await mapConcurrent(FAST_SCAN_TARGETS, options.scanConcurrency, async (target) => {
    totalEntries += await scanStorageEntries(api, target, records, options.json);
  });

  if (!options.json) {
    console.log(`[scan] fast totalEntries=${totalEntries} storages=${FAST_SCAN_TARGETS.length}`);
  }

  return [...records.values()].sort((a, b) => a.cid.localeCompare(b.cid));
}

async function collectAllOnChainCids(
  api: ApiPromise,
  options: Pick<CliOptions, 'json' | 'palletFilter' | 'scanConcurrency'>,
): Promise<CidRecord[]> {
  const records = new Map<string, CidRecord>();
  const queryRoot = api.query as Record<string, unknown>;
  const targets: StorageScanTarget[] = [];

  for (const [pallet, section] of Object.entries(queryRoot)) {
    if (pallet.startsWith('$') || typeof section !== 'object' || section == null) {
      continue;
    }
    if (!shouldScanPallet(pallet, options.palletFilter)) {
      continue;
    }

    for (const [storage, query] of Object.entries(section as Record<string, any>)) {
      if (query && typeof query.entries === 'function') {
        targets.push({ pallet, storage });
      }
    }
  }

  await mapConcurrent(targets, options.scanConcurrency, async (target) => {
    await scanStorageEntries(api, target, records, options.json);
  });

  return [...records.values()].sort((a, b) => a.cid.localeCompare(b.cid));
}

async function collectCids(api: ApiPromise, options: CliOptions): Promise<CidRecord[]> {
  if (options.scanMode === 'registry') {
    return collectActiveRegistryCids(api, options.scanConcurrency);
  }
  if (options.scanMode === 'fast') {
    return collectFastOnChainCids(api, options);
  }
  return collectAllOnChainCids(api, options);
}

async function pinCidOnce(cid: string, options: CliOptions): Promise<PinResult> {
  const endpoint = new URL(options.pinEndpoint!);
  const headers: Record<string, string> = {};
  let body: string | undefined;

  if (options.pinToken) {
    headers.authorization = `Bearer ${options.pinToken}`;
  }

  if (endpoint.hostname.includes('pinata.cloud')) {
    headers['content-type'] = 'application/json';
    body = JSON.stringify({ hashToPin: cid });
  } else {
    endpoint.searchParams.set('arg', cid);
    endpoint.searchParams.set('recursive', 'true');
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), options.pinTimeoutMs);
  try {
    const response = await fetch(endpoint, {
      method: 'POST',
      headers,
      body,
      signal: controller.signal,
    });
    if (!response.ok) {
      const text = await response.text().catch(() => '');
      return { cid, ok: false, status: response.status, error: text.slice(0, 300) };
    }
    return { cid, ok: true, status: response.status };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { cid, ok: false, error: message };
  } finally {
    clearTimeout(timer);
  }
}

async function pinCidWithRetry(cid: string, options: CliOptions): Promise<PinResult> {
  if (options.dryRun) {
    return { cid, ok: true, attempts: 0 };
  }

  let last: PinResult = { cid, ok: false, error: 'no attempts' };
  for (let attempt = 0; attempt <= options.pinRetries; attempt++) {
    if (attempt > 0) {
      await sleep(1000 * attempt);
    }
    last = await pinCidOnce(cid, options);
    last.attempts = attempt + 1;
    if (last.ok) {
      return last;
    }
  }
  return last;
}

async function mapConcurrent<T, R>(
  items: T[],
  concurrency: number,
  fn: (item: T, index: number) => Promise<R>,
): Promise<R[]> {
  if (items.length === 0) {
    return [];
  }

  const results = new Array<R>(items.length);
  let nextIndex = 0;

  async function worker(): Promise<void> {
    while (nextIndex < items.length) {
      const current = nextIndex++;
      results[current] = await fn(items[current], current);
    }
  }

  await Promise.all(
    Array.from({ length: Math.min(concurrency, items.length) }, () => worker()),
  );
  return results;
}

async function writeJsonAtomic(path: string, data: unknown): Promise<void> {
  const tmpPath = `${path}.tmp`;
  await writeFile(tmpPath, `${JSON.stringify(data, null, 2)}\n`, 'utf8');
  await rename(tmpPath, path);
}

async function notifyPinFailures(summary: CycleSummary): Promise<void> {
  const webhookUrl = process.env.ALERT_WEBHOOK_URL?.trim();
  if (!webhookUrl || summary.failed <= 0 || summary.dryRun) {
    return;
  }

  const payload = {
    event: 'nexus.pin_onchain_cids.failed',
    failed: summary.failed,
    cidCount: summary.cidCount,
    pinned: summary.pinned,
    scanMode: summary.scanMode,
    outputFile: summary.outputFile,
    finishedAt: summary.finishedAt,
    failures: summary.cids.filter((item) => !item.ok).map((item) => ({
      cid: item.cid,
      error: item.error,
      status: item.status,
    })),
  };

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10_000);
  try {
    const response = await fetch(webhookUrl, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(payload),
      signal: controller.signal,
    });
    if (!response.ok) {
      console.warn(`[alert-warning] webhook status=${response.status}`);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.warn(`[alert-warning] webhook failed: ${message}`);
  } finally {
    clearTimeout(timer);
  }
}

async function pinCollectedCids(
  cids: CidRecord[],
  options: CliOptions,
): Promise<Array<CidRecord & PinResult>> {
  return mapConcurrent(cids, options.pinConcurrency, async (record) => {
    const result = await pinCidWithRetry(record.cid, options);
    if (!options.json) {
      const prefix = options.dryRun ? 'dry-run' : result.ok ? 'pinned' : 'failed';
      console.log(
        `[${prefix}] ${record.cid} sources=${record.sources.length}`
        + `${result.attempts ? ` attempts=${result.attempts}` : ''}`
        + `${result.error ? ` ${result.error}` : ''}`,
      );
    }
    return { ...record, ...result };
  });
}

async function runQueryPinCycle(api: ApiPromise, options: CliOptions): Promise<CycleSummary> {
  const startedAt = new Date().toISOString();

  if (!options.json) {
    console.log('[phase] query (fast scan)');
  }
  const queryCids = await collectFastOnChainCids(api, options);
  const querySummary = {
    startedAt,
    finishedAt: new Date().toISOString(),
    wsUrl: process.env.WS_URL,
    phase: 'query',
    scanMode: 'fast',
    outputFile: options.queryOutputFile,
    cidCount: queryCids.length,
    cids: queryCids,
  };
  await writeJsonAtomic(options.queryOutputFile, querySummary);
  if (!options.json) {
    console.log(`[query-summary] cids=${queryCids.length} output=${options.queryOutputFile}`);
    console.log('[phase] pin (active registry)');
  }

  const pinCids = await collectActiveRegistryCids(api, options.scanConcurrency);
  const pinResults = await pinCollectedCids(pinCids, options);
  const failed = pinResults.filter((item) => !item.ok).length;
  const summary: CycleSummary = {
    startedAt,
    finishedAt: new Date().toISOString(),
    wsUrl: process.env.WS_URL,
    dryRun: options.dryRun,
    scanMode: 'query-pin',
    fullScan: options.fullScan,
    failOnError: options.failOnError,
    outputFile: options.outputFile,
    queryOutputFile: options.queryOutputFile,
    queryCidCount: queryCids.length,
    cidCount: pinCids.length,
    pinned: pinResults.length - failed,
    failed,
    cids: pinResults,
  };

  await writeJsonAtomic(options.outputFile, summary);
  await notifyPinFailures(summary);

  if (options.json) {
    console.log(JSON.stringify(summary, null, 2));
  } else {
    console.log(
      `[summary] mode=query-pin query=${summary.queryCidCount} pin=${summary.cidCount}`
      + ` ok=${summary.pinned} failed=${summary.failed}`
      + ` queryOut=${options.queryOutputFile} pinOut=${options.outputFile}`,
    );
  }

  return summary;
}

async function runCycle(api: ApiPromise, options: CliOptions): Promise<CycleSummary> {
  if (options.scanMode === 'query-pin') {
    return runQueryPinCycle(api, options);
  }

  const startedAt = new Date().toISOString();
  const cids = await collectCids(api, options);
  const pinResults = await pinCollectedCids(cids, options);

  const failed = pinResults.filter((item) => !item.ok).length;
  const summary: CycleSummary = {
    startedAt,
    finishedAt: new Date().toISOString(),
    wsUrl: process.env.WS_URL,
    dryRun: options.dryRun,
    scanMode: options.scanMode,
    fullScan: options.fullScan,
    palletFilter: options.palletFilter ? [...options.palletFilter] : undefined,
    failOnError: options.failOnError,
    outputFile: options.outputFile,
    cidCount: cids.length,
    pinned: pinResults.length - failed,
    failed,
    cids: pinResults,
  };

  await writeJsonAtomic(options.outputFile, summary);
  await notifyPinFailures(summary);

  if (options.json) {
    console.log(JSON.stringify(summary, null, 2));
  } else {
    console.log(
      `[summary] mode=${summary.scanMode} cids=${summary.cidCount}`
      + ` ok=${summary.pinned} failed=${summary.failed} output=${options.outputFile}`,
    );
  }

  return summary;
}

function warnUnsafePinMode(options: CliOptions): void {
  if (options.dryRun || options.scanMode !== 'all') {
    return;
  }
  console.warn(
    '[warn] --scan-mode all pins every discovered CID on-chain; use registry mode for production pinning',
  );
}

async function connectApiWithTimeout(wsUrl: string, timeoutMs: number, json: boolean): Promise<ApiPromise> {
  if (!json) {
    console.log(`[connecting] ${wsUrl} timeout=${timeoutMs}ms`);
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      connectApi(wsUrl),
      new Promise<ApiPromise>((_, reject) => {
        timer = setTimeout(() => {
          reject(new Error(
            `RPC connection timed out after ${timeoutMs}ms (${wsUrl}). `
            + 'Check network/firewall, or use a local node: '
            + 'WS_URL=ws://127.0.0.1:9944 ./target/release/nexus-node --dev',
          ));
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timer) {
      clearTimeout(timer);
    }
  }
}

async function main(): Promise<void> {
  const options = parseCli(process.argv.slice(2));
  warnUnsafePinMode(options);

  if (!options.json) {
    console.log(
      `[scan] mode=${options.scanMode}`
      + `${options.fullScan ? ' full-scan' : ''}`
      + `${options.scanMode === 'all' && options.palletFilter ? ` pallets=${[...options.palletFilter].join(',')}` : ''}`
      + `${options.scanMode === 'fast' || options.scanMode === 'query-pin' ? ` storages=${FAST_SCAN_TARGETS.length}` : ''}`
      + ` scanConcurrency=${options.scanConcurrency}`,
    );
    console.log(`[loop] interval=${formatInterval(options.intervalMs)}${options.once ? ' once' : ''}`);
    console.log(`[pin] ${options.dryRun ? 'dry-run' : options.pinEndpoint}`);
    if (options.failOnError) {
      console.log('[exit] fail-on-error enabled');
    }
    if (process.env.INSECURE_TLS === '1') {
      console.warn('[warn] INSECURE_TLS=1 disables TLS certificate verification');
    }
  }

  do {
    const api = await connectApiWithTimeout(process.env.WS_URL!, options.wsConnectTimeoutMs, options.json);
    try {
      if (!options.json) {
        const snapshot = await captureChainSnapshot(api);
        console.log(
          `[chain] ${snapshot.chain} ${snapshot.nodeName} ${snapshot.nodeVersion}`
          + ` spec=${snapshot.specName}/${snapshot.specVersion}`,
        );
      }
      const summary = await runCycle(api, options);
      if (options.failOnError && summary.failed > 0) {
        console.error(`[error] ${summary.failed} pin operation(s) failed`);
        process.exitCode = 1;
      }
    } finally {
      await disconnectApi(api);
    }

    if (options.once) {
      break;
    }
    if (!options.json) {
      console.log(`[sleep] waiting ${formatInterval(options.intervalMs)} until next cycle`);
    }
    await sleep(options.intervalMs);
  } while (true);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
