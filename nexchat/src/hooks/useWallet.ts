// EN: Desktop wallet hook — unlock, dev shortcut, lock. Wires keyring → chain signer → app session.
// CN: 桌面钱包 hook——解锁、dev 快捷入口、锁定；串联 keyring → 链签名者 → 应用会话。

import { useCallback } from "react";
import { chainClient } from "@/chain/chainClient";
import { config } from "@/config";
import { useAppStore } from "@/state/appStore";
import { useWalletStore } from "@/state/walletStore";
import { canonicalAddress } from "@/wallet/address";
import {
  createAccount,
  importAccount,
  listAccounts,
  deleteAccount,
  type DesktopAccount,
} from "@/wallet/desktopKeyring";
import { lockDesktopSession, unlockDesktopSession } from "@/wallet/session";

export function useWallet() {
  const { address, name, source, isConnected, setWallet, disconnect } = useWalletStore();
  const unlockApp = useAppStore((s) => s.unlock);

  const loadAccounts = useCallback(async (): Promise<DesktopAccount[]> => listAccounts(), []);

  const unlockAndEnter = useCallback(
    async (accountAddress: string, password: string, displayName?: string) => {
      await unlockDesktopSession(accountAddress, password);
      try {
        const label =
          displayName ??
          (await listAccounts()).find((a) => a.address === accountAddress)?.name ??
          "Account";
        await unlockApp(canonicalAddress(accountAddress), label, { mode: "desktop" });
        setWallet(accountAddress, label, "desktop-keyring");
      } catch (e) {
        lockDesktopSession();
        throw e;
      }
    },
    [setWallet, unlockApp],
  );

  const enterWithDev = useCallback(async () => {
    if (!config.devWallet) throw new Error("Dev wallet disabled (VITE_DEV_WALLET=false)");
    const devAddr = await chainClient.useDevAccount(config.devSeed);
    const label = config.devSeed.replace(/^\/\//, "") || "Dev";
    setWallet(devAddr, label, "dev");
    await unlockApp(canonicalAddress(devAddr), label, { mode: "dev" });
  }, [setWallet, unlockApp]);

  const createAndEnter = useCallback(
    async (accName: string, password: string) => {
      const { mnemonic, address } = await createAccount(accName, password);
      await unlockAndEnter(address, password, accName);
      return { mnemonic, address };
    },
    [unlockAndEnter],
  );

  const importAndEnter = useCallback(
    async (mnemonic: string, accName: string, password: string) => {
      const { address } = await importAccount(mnemonic, accName, password);
      await unlockAndEnter(address, password, accName);
      return { address };
    },
    [unlockAndEnter],
  );

  const lock = useCallback(() => {
    lockDesktopSession();
    disconnect();
    window.location.reload();
  }, [disconnect]);

  /** EN: Prefer another cached account — lock session and reload to unlock screen. CN: 切换首选账户——锁定后刷新到解锁页。 */
  const preferAccount = useCallback(
    (accountAddress: string, displayName: string) => {
      lockDesktopSession();
      setWallet(accountAddress, displayName, "desktop-keyring");
      window.location.reload();
    },
    [setWallet],
  );

  const removeAccount = useCallback(async (accountAddress: string) => {
    await deleteAccount(accountAddress);
    if (address === accountAddress) disconnect();
  }, [address, disconnect]);

  const isUnlocked = chainClient.signerAddress != null;

  return {
    address,
    name,
    source,
    isConnected,
    isUnlocked,
    loadAccounts,
    unlockAndEnter,
    enterWithDev,
    createAndEnter,
    importAndEnter,
    lock,
    removeAccount,
    preferAccount,
  };
}
