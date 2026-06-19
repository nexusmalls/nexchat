// EN: Shared cloud-pointer put with stale_updated_at recovery (refetch + local merge).
// CN: 云指针 put 共用逻辑，含 stale_updated_at 拒绝后的 refetch + 本地合并。

import { relayOneShotSend } from "@/relay/relayOneShot";
import { RelayStalePointerError } from "@/relay/relayErrors";

export type CloudPointer = { cid: string; updated_at: number };

/// EN: Put pointer; on remote LWW win, adopt relay copy into local storage. CN: 发布指针；远端 LWW
/// 胜出时拉取并写入本地。
export async function publishCloudPointer<T extends CloudPointer>(
  account: string,
  putType: string,
  ackType: string,
  ptr: T,
  writeLocal: (account: string, ptr: T) => void,
  fetchRemote: (account: string) => Promise<T | null>,
): Promise<void> {
  try {
    await relayOneShotSend(
      account,
      { type: putType, account, cid: ptr.cid, updated_at: ptr.updated_at },
      { ackType },
    );
  } catch (e) {
    if (!(e instanceof RelayStalePointerError)) throw e;
    const remote = await fetchRemote(account);
    if (remote) writeLocal(account, remote);
  }
}
