// EN: Persisted desktop-wallet selection (address/name/source). Signer lives in chain/signer.
// CN: 持久化的桌面钱包选择（地址/名称/来源）；签名者在 chain/signer。

import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";

export type WalletSource = "desktop-keyring" | "dev";

interface WalletState {
  address: string | null;
  name: string | null;
  source: WalletSource | null;
  isConnected: boolean;

  setWallet: (address: string, name: string, source: WalletSource) => void;
  disconnect: () => void;
}

const storage = createJSONStorage(() =>
  typeof localStorage !== "undefined"
    ? localStorage
    : {
        getItem: () => null,
        setItem: () => {},
        removeItem: () => {},
      },
);

export const useWalletStore = create<WalletState>()(
  persist(
    (set) => ({
      address: null,
      name: null,
      source: null,
      isConnected: false,

      setWallet: (address, name, source) =>
        set({ address, name, source, isConnected: true }),

      disconnect: () =>
        set({ address: null, name: null, source: null, isConnected: false }),
    }),
    {
      name: "nexchat-wallet",
      storage,
      partialize: (s) => ({
        address: s.address,
        name: s.name,
        source: s.source,
      }),
      merge: (persisted, current) => {
        const p = persisted as Partial<Pick<WalletState, "address" | "name" | "source">> | null;
        if (!p?.address || !p.source) return current;
        return {
          ...current,
          address: p.address,
          name: p.name ?? null,
          source: p.source,
          isConnected: true,
        };
      },
    },
  ),
);
