// EN: Active chain signer state (built-in desktop keyring pair; legacy injector optional).
// CN: 当前链上签名者状态（内置 desktop keyring pair；注入器为遗留可选路径）。

import type { KeyringPair } from "@polkadot/keyring/types";
import type { Signer } from "@polkadot/types/types";

export type SignBackend =
  | { kind: "pair"; address: string; pair: KeyringPair }
  | { kind: "injector"; address: string; signer: Signer };

let active: SignBackend | null = null;

export function setSignerPair(pair: KeyringPair): void {
  active = { kind: "pair", address: pair.address, pair };
}

export function setInjectorSigner(address: string, signer: Signer): void {
  active = { kind: "injector", address, signer };
}

export function clearSigner(): void {
  active = null;
}

export function getSignerAddress(): string | null {
  return active?.address ?? null;
}

export function getSignBackend(): SignBackend | null {
  return active;
}

export function requireSigner(): SignBackend {
  if (!active) throw new Error("链上签名者未配置（请先解锁钱包或启用 dev 钱包）");
  if (active.kind === "pair" && active.pair.isLocked) {
    throw new Error("钱包已锁定，请重新输入密码以签名");
  }
  return active;
}

/// EN: Sign raw bytes with the active account SS58 key (sr25519) for the E2EI device-leaf credential
/// (§3.9). Production uses the built-in desktop keyring pair from WalletGate (`sign` verbatim,
/// verifiable via `signatureVerify`). Injector wallets are legacy/unused in the main UI: external
/// extensions may wrap raw payloads (`<Bytes>…</Bytes>`), so this returns null for injectors.
/// CN: 用当前账户 SS58 钥签名裸字节，供 E2EI 设备 leaf 凭证（§3.9）。生产路径为 WalletGate 解锁的
/// 内置 desktop keyring pair（原样签名，可 `signatureVerify` 验证）。注入器为遗留/主 UI 未使用路径。
export function signRawWithAccountKey(bytes: Uint8Array): Uint8Array | null {
  if (active?.kind === "pair") return active.pair.sign(bytes);
  return null;
}
