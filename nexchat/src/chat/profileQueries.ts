// EN: Read `chatCore` user profile from chain storage.
// CN: 从链上 storage 读取 `chatCore` 用户资料。

import type { ChatUserProfileView } from "@/chat/profileTypes";
import { canonicalAddress } from "@/wallet/address";

type ProfileApi = {
  query: {
    chatCore: {
      accountToChatUserId: (who: string) => Promise<StorageOption>;
      chatUserProfiles: (id: unknown) => Promise<StorageOption>;
    };
  };
};

type StorageOption = {
  isNone?: boolean;
  isSome?: boolean;
  unwrap?: () => unknown;
};

function readUtf8Field(raw: unknown): string | null {
  if (raw == null) return null;
  const o = raw as StorageOption;
  if (o.isNone) return null;
  if (o.isSome && o.unwrap) return readUtf8Field(o.unwrap());
  const v = raw as { toUtf8?: () => string; toString?: () => string };
  if (typeof v.toUtf8 === "function") {
    const s = v.toUtf8();
    return s.length > 0 ? s : null;
  }
  if (typeof v.toString === "function") {
    const s = v.toString();
    if (s.startsWith("0x") && s.length > 2) {
      try {
        const bytes = new Uint8Array(
          s
            .slice(2)
            .match(/.{1,2}/g)!
            .map((h) => parseInt(h, 16)),
        );
        const decoded = new TextDecoder().decode(bytes);
        return decoded.length > 0 ? decoded : null;
      } catch {
        return null;
      }
    }
    return s.length > 0 ? s : null;
  }
  return null;
}

// EN: Load chat profile for an SS58 account (null if never registered on-chain).
// CN: 按 SS58 账户加载聊天资料（未上链注册时返回 null）。
export async function fetchChatUserProfile(
  api: ProfileApi,
  addressRaw: string,
): Promise<ChatUserProfileView | null> {
  const who = canonicalAddress(addressRaw);
  const idOpt = await api.query.chatCore.accountToChatUserId(who);
  if (idOpt?.isNone) return null;

  const chatUserId = idOpt.unwrap?.() ?? idOpt;
  const profOpt = await api.query.chatCore.chatUserProfiles(chatUserId);
  if (profOpt?.isNone) return null;

  const prof = (profOpt.unwrap?.() ?? profOpt) as Record<string, unknown>;
  return {
    nickname: readUtf8Field(prof.nickname),
    avatarCid: readUtf8Field(prof.avatarCid ?? prof.avatar_cid),
    signature: readUtf8Field(prof.signature),
  };
}
