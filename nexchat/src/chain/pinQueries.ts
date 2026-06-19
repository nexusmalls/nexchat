// EN: Read-model for `pallet-storage-service` pins owned by an account (renew_pin UI).
// CN: 账户名下 `pallet-storage-service` Pin 的读模型（续费 UI）。

export type PinGrace = "normal" | "inGrace" | "expired";

export interface PinRow {
  cidHash: string;
  cid: string;
  sizeBytes: number;
  replicas: number;
  dueBlock: number;
  grace: PinGrace;
  graceExpiresBlock?: number;
}

/// EN: Decode plaintext CID bytes from chain storage. CN: 解码链上存的明文 CID 字节。
export function cidBytesToString(raw: Uint8Array): string {
  return new TextDecoder().decode(raw);
}
