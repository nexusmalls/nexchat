import { useCallback, useEffect, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { fetchChatUserProfile } from "@/chat/profileQueries";
import type { ChatUserProfileView } from "@/chat/profileTypes";

// EN: Poll on-chain chat user profile (avatar CID, nickname).
// CN: 轮询链上聊天用户资料（头像 CID、昵称）。
export function useChatProfile(address: string | null, enabled = true) {
  const [profile, setProfile] = useState<ChatUserProfileView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled || config.useMock || !address) {
      setProfile(null);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchChatUserProfile
      >[0];
      setProfile(await fetchChatUserProfile(api, address));
    } catch (e) {
      setProfile(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [enabled, address]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { profile, loading, error, refresh };
}
