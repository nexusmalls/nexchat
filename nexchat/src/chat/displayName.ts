// EN: Chat display names — nickname + last 4 chars of address (no full SS58).
// CN: 聊天展示名——昵称 + 地址后四位（不显示完整地址）。

import type { MentionMember } from "@/p3/mentions";
import { canonicalAddress, nexDisplayAddress } from "@/wallet/address";
import { decodeAddress } from "@polkadot/util-crypto";

export function isSs58Address(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return false;
  try {
    decodeAddress(trimmed);
    return true;
  } catch {
    return false;
  }
}

function addressSuffix(address: string): string {
  return nexDisplayAddress(address).slice(-4);
}

function lookupNickname(
  address: string,
  roster: readonly MentionMember[],
  selfNickname?: string,
  selfAddress?: string,
): string {
  const canon = canonicalAddress(address);
  if (selfAddress && canon === canonicalAddress(selfAddress)) {
    return selfNickname?.trim() || "我";
  }
  const hit = roster.find((m) => canonicalAddress(m.address) === canon);
  return hit?.ref?.trim() || "用户";
}

function formatWithSuffix(nickname: string, address?: string): string {
  const name = nickname.trim() || "用户";
  if (!address || !isSs58Address(address)) return name;
  return `${name}·${addressSuffix(address)}`;
}

export interface ChatDisplayNameOptions {
  selfNickname?: string;
  selfAddress?: string;
  /** EN: Known on-chain address when `senderRef` is a nickname. CN: senderRef 为昵称时的链上地址。 */
  fallbackAddress?: string;
}

// EN: Resolve UI label for chat header / bubbles / reply bars.
// CN: 解析聊天顶栏、气泡、回复条展示名。
export function formatChatDisplayName(
  senderRef: string,
  roster: readonly MentionMember[],
  options?: ChatDisplayNameOptions,
): string {
  const ref = senderRef.trim();
  if (!ref) return "用户";

  if (ref === "me") {
    return formatWithSuffix(options?.selfNickname ?? "我", options?.selfAddress);
  }

  if (isSs58Address(ref)) {
    const nick = lookupNickname(ref, roster, options?.selfNickname, options?.selfAddress);
    return formatWithSuffix(nick, ref);
  }

  const member = roster.find(
    (m) => m.ref === ref || m.labels.some((label) => label === ref),
  );
  if (member) {
    return formatWithSuffix(member.ref, member.address);
  }

  if (options?.fallbackAddress) {
    return formatWithSuffix(ref, options.fallbackAddress);
  }

  return ref;
}
