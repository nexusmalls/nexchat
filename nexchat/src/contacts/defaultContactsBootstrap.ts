// EN: Auto-add configured default friends on unlock; labels prefer on-chain chat nicknames.
// CN: 解锁时自动添加配置的默认好友；显示名优先使用链上聊天昵称。

import { fetchChatUserProfile } from "@/chat/profileQueries";
import { chainClient } from "@/chain/chainClient";
import { config } from "@/config";
import { loadContacts, parseContactAddress } from "@/store/contactBook";
import { canonicalAddress, shortAddress } from "@/wallet/address";

type ProfileApi = Parameters<typeof fetchChatUserProfile>[0];

export type DefaultContactAdder = (
  peer: string,
  label: string,
  opts?: { notify?: boolean },
) => Promise<void>;

/// EN: Pick a contact label — chain nickname, then optional seed name, then short address.
/// CN: 选择联系人显示名——链上昵称 → 可选 seed 名 → 短地址。
export function resolveDefaultContactLabel(
  address: string,
  profile: { nickname: string | null } | null | undefined,
  seedFallback?: string,
): string {
  const nick = profile?.nickname?.trim();
  if (nick) return nick;
  const seed = seedFallback?.trim();
  if (seed) return seed;
  return shortAddress(address);
}

/// EN: Addresses to bootstrap as saved contacts (deduped, excludes self and existing).
/// CN: 应写入通讯录的默认地址（去重，排除自己与已有联系人）。
export function collectDefaultContactCandidates(
  selfAddress: string,
  existingAddresses: readonly string[],
  configuredAddresses: readonly string[],
  rosterAddresses: readonly string[],
): string[] {
  const self = canonicalAddress(selfAddress);
  const existing = new Set(existingAddresses.map(canonicalAddress));
  const out: string[] = [];
  const seen = new Set<string>();
  for (const raw of [...configuredAddresses, ...rosterAddresses]) {
    let addr: string;
    try {
      addr = parseContactAddress(raw);
    } catch {
      continue;
    }
    if (addr === self || existing.has(addr) || seen.has(addr)) continue;
    seen.add(addr);
    out.push(addr);
  }
  return out;
}

async function seedLabelByAddress(): Promise<Map<string, string>> {
  const map = new Map<string, string>();
  if (config.mlsRosterSeeds.length === 0) return map;
  const addrs = await Promise.all(
    config.mlsRosterSeeds.map((seed) => chainClient.deriveAddress(seed)),
  );
  for (let i = 0; i < config.mlsRosterSeeds.length; i++) {
    const seed = config.mlsRosterSeeds[i]!;
    const addr = addrs[i];
    if (!addr) continue;
    map.set(canonicalAddress(addr), seed.replace(/^\/\//, ""));
  }
  return map;
}

async function resolveConfiguredDefaultAddresses(): Promise<string[]> {
  const out: string[] = [];
  for (const raw of config.defaultContactAddresses) {
    try {
      out.push(parseContactAddress(raw));
    } catch {
      console.warn("[nexchat] invalid VITE_DEFAULT_CONTACTS entry:", raw);
    }
  }
  return out;
}

async function resolveRosterDefaultAddresses(): Promise<string[]> {
  if (config.mlsRosterSeeds.length === 0) return [];
  const addrs = await Promise.all(
    config.mlsRosterSeeds.map((seed) => chainClient.deriveAddress(seed)),
  );
  return addrs.map(canonicalAddress);
}

/// EN: Add missing default friends; returns count added. Skips mock mode and notify handshake.
/// CN: 补齐缺失的默认好友；返回新增数量。mock 模式跳过，且不发送联系人请求通知。
export async function ensureDefaultContacts(
  selfAddress: string,
  addContact: DefaultContactAdder,
): Promise<number> {
  if (config.useMock) return 0;

  const configured = await resolveConfiguredDefaultAddresses();
  const roster = await resolveRosterDefaultAddresses();
  if (configured.length === 0 && roster.length === 0) return 0;

  const existing = loadContacts(selfAddress).map((c) => c.address);
  const candidates = collectDefaultContactCandidates(
    selfAddress,
    existing,
    configured,
    roster,
  );
  if (candidates.length === 0) return 0;

  const seedLabels = await seedLabelByAddress();
  let api: ProfileApi | null = null;
  try {
    api = (await chainClient.getApiForWallet()) as unknown as ProfileApi;
  } catch (e) {
    console.warn("[nexchat] default contacts: chain profile fetch unavailable:", e);
  }

  let added = 0;
  for (const addr of candidates) {
    let profile: Awaited<ReturnType<typeof fetchChatUserProfile>> = null;
    if (api) {
      try {
        profile = await fetchChatUserProfile(api, addr);
      } catch (e) {
        console.warn("[nexchat] default contacts: profile fetch failed for", addr, e);
      }
    }
    const label = resolveDefaultContactLabel(addr, profile, seedLabels.get(addr));
    await addContact(addr, label, { notify: false });
    added++;
  }
  return added;
}
