// EN: Connect an unlocked desktop keyring account to ChainClient signing.
// CN: 将已解锁的桌面 keyring 账户接到 ChainClient 签名路径。

import { chainClient } from "@/chain/chainClient";
import { setSignerPair } from "@/chain/signer";
import { keyVault } from "@/keyvault/keyvault";
import { unlockAccount } from "@/wallet/desktopKeyring";
import { clearVaultMaster, deriveVaultMasterFromPair, setVaultMaster } from "@/wallet/vaultMaster";

/// EN: Unlock keystore and install as active chain signer; also derives the account
/// `vault_master` (ADR CHAT_SYNC_ANCHOR §5.0) from the unlocked pair. Returns SS58 address.
/// CN: 解锁 keystore 并设为当前链上签名者；同时从已解锁 pair 派生账户 `vault_master`
/// （ADR CHAT_SYNC_ANCHOR §5.0）。返回 SS58 地址。
export async function unlockDesktopSession(address: string, password: string): Promise<string> {
  const { pair } = await unlockAccount(address, password, chainClient.getApiForWallet);
  setSignerPair(pair);
  setVaultMaster(await deriveVaultMasterFromPair(pair));
  return pair.address;
}

export function lockDesktopSession(): void {
  chainClient.disconnectSigner();
  clearVaultMaster();
  keyVault.clear();
}
