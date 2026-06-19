// EN: Batch-fetch on-chain profile avatar CIDs for peer addresses.
// CN: 批量拉取对端链上资料头像 CID。

import { fetchChatUserProfile } from "@/chat/profileQueries";
import { chainClient } from "@/chain/chainClient";
import { config } from "@/config";
import { canonicalAddress } from "@/wallet/address";

export async function fetchPeerAvatarMap(
  addresses: readonly string[],
): Promise<Map<string, string>> {
  const out = new Map<string, string>();
  if (config.useMock || addresses.length === 0) return out;

  const unique = [...new Set(addresses.map((a) => canonicalAddress(a)).filter(Boolean))];
  if (unique.length === 0) return out;

  try {
    const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
      typeof fetchChatUserProfile
    >[0];
    await Promise.all(
      unique.map(async (addr) => {
        try {
          const prof = await fetchChatUserProfile(api, addr);
          if (prof?.avatarCid) out.set(addr, prof.avatarCid);
        } catch {
          /* skip peer */
        }
      }),
    );
  } catch {
    /* chain unavailable */
  }
  return out;
}
