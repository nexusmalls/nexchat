import { useEffect, useMemo, useState } from "react";
import { fetchPeerAvatarMap } from "@/chat/peerAvatars";
import { config } from "@/config";
import { canonicalAddress } from "@/wallet/address";

// EN: Cached map of peer SS58 → avatar CID (on-chain chat profile).
// CN: 对端 SS58 → 头像 CID 缓存（链上 chat 资料）。
export function usePeerAvatarMap(addresses: readonly string[], enabled = true): Map<string, string> {
  const [map, setMap] = useState<Map<string, string>>(new Map());
  const key = useMemo(
    () => [...new Set(addresses.map((a) => canonicalAddress(a)))].sort().join(","),
    [addresses],
  );

  useEffect(() => {
    if (!enabled || config.useMock || !key) {
      setMap(new Map());
      return;
    }
    let alive = true;
    void fetchPeerAvatarMap(key.split(",").filter(Boolean)).then((next) => {
      if (alive) setMap(next);
    });
    return () => {
      alive = false;
    };
  }, [enabled, key]);

  return map;
}

export function peerAvatarCid(
  map: Map<string, string>,
  address: string | undefined,
): string | undefined {
  if (!address) return undefined;
  return map.get(canonicalAddress(address));
}
