// EN: Built-in desktop keyring — create/import/unlock without browser extension.
// Ported from nexus-entity-dapp; browser-only (localStorage), SS58 prefix 273 (NEX).
// CN: 内置桌面 keyring——无需浏览器扩展即可创建/导入/解锁；源自 entity-dapp。

import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady, mnemonicGenerate } from "@polkadot/util-crypto";
import type { KeyringPair } from "@polkadot/keyring/types";
import type { Signer, SignerResult } from "@polkadot/types/types";
import type { SignerPayloadJSON, SignerPayloadRaw } from "@polkadot/types/types";

const LS_PREFIX = "nexchat-keystore:";
const LS_MNEMONIC_PREFIX = "nexchat-keystore-mnemonic:";

/** EN: Nexus chain SS58 prefix (273). CN: Nexus 链 SS58 前缀（273）。 */
export const NEX_SS58 = 273;

export interface DesktopAccount {
  address: string;
  name: string;
  encoded: string;
}

export type ApiFactory = () => Promise<{
  registry: { createType: (type: string, value: unknown, options?: unknown) => { sign: (pair: KeyringPair) => { signature: string } } };
}>;

function lsKey(address: string): string {
  return `${LS_PREFIX}${address}`;
}

function lsWriteAccount(address: string, json: object): void {
  localStorage.setItem(lsKey(address), JSON.stringify(json));
}

function lsReadAccount(address: string): string | null {
  return localStorage.getItem(lsKey(address));
}

function lsListAccounts(): { address: string; content: string }[] {
  const results: { address: string; content: string }[] = [];
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key?.startsWith(LS_PREFIX) && !key.startsWith(LS_MNEMONIC_PREFIX)) {
      const content = localStorage.getItem(key);
      if (content) results.push({ address: key.slice(LS_PREFIX.length), content });
    }
  }
  return results;
}

function lsDeleteAccount(address: string): void {
  localStorage.removeItem(lsKey(address));
}

function lsMnemonicKey(address: string): string {
  return `${LS_MNEMONIC_PREFIX}${address}`;
}

function lsWriteMnemonic(address: string, encrypted: string): void {
  localStorage.setItem(lsMnemonicKey(address), JSON.stringify({ encrypted }));
}

function lsReadMnemonic(address: string): string | null {
  return localStorage.getItem(lsMnemonicKey(address));
}

function lsDeleteMnemonic(address: string): void {
  localStorage.removeItem(lsMnemonicKey(address));
}

function isWebCryptoAvailable(): boolean {
  return typeof crypto !== "undefined" && typeof crypto.subtle !== "undefined";
}

function assertLocalStorage(): void {
  if (typeof localStorage === "undefined") {
    throw new Error("浏览器不支持 localStorage，无法保存钱包");
  }
  try {
    const probe = "__nexchat_ls_probe__";
    localStorage.setItem(probe, "1");
    localStorage.removeItem(probe);
  } catch {
    throw new Error("localStorage 不可用（请关闭隐私模式或使用 localhost/https）");
  }
}

function u8aToB64(bytes: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]!);
  return btoa(bin);
}

async function deriveEncryptionKey(password: string, salt: Uint8Array): Promise<CryptoKey> {
  const encoder = new TextEncoder();
  const keyMaterial = await crypto.subtle.importKey(
    "raw",
    encoder.encode(password),
    "PBKDF2",
    false,
    ["deriveKey"],
  );
  return crypto.subtle.deriveKey(
    { name: "PBKDF2", salt: salt as BufferSource, iterations: 100_000, hash: "SHA-256" },
    keyMaterial,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

async function encryptMnemonicWebCrypto(mnemonic: string, password: string): Promise<string> {
  const encoder = new TextEncoder();
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await deriveEncryptionKey(password, salt);
  const ciphertext = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: iv as BufferSource },
    key,
    encoder.encode(mnemonic),
  );
  const ct = new Uint8Array(ciphertext);
  const packed = new Uint8Array(salt.length + iv.length + ct.length);
  packed.set(salt, 0);
  packed.set(iv, salt.length);
  packed.set(ct, salt.length + iv.length);
  return u8aToB64(packed);
}

async function decryptMnemonicWebCrypto(encrypted: string, password: string): Promise<string> {
  const packed = Uint8Array.from(atob(encrypted), (c) => c.charCodeAt(0));
  const salt = packed.slice(0, 16);
  const iv = packed.slice(16, 28);
  const ciphertext = packed.slice(28);
  const key = await deriveEncryptionKey(password, salt);
  const plaintext = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: iv as BufferSource },
    key,
    ciphertext,
  );
  return new TextDecoder().decode(plaintext);
}

async function deriveNaclKey(password: string, salt: Uint8Array): Promise<Uint8Array> {
  const { blake2AsU8a } = await import("@polkadot/util-crypto");
  const encoder = new TextEncoder();
  const pwBytes = encoder.encode(password);
  const input = new Uint8Array(pwBytes.length + salt.length);
  input.set(pwBytes, 0);
  input.set(salt, pwBytes.length);
  return blake2AsU8a(input, 256);
}

async function encryptMnemonicFallback(mnemonic: string, password: string): Promise<string> {
  const { naclEncrypt, randomAsU8a } = await import("@polkadot/util-crypto");
  const salt = randomAsU8a(16);
  const secret = await deriveNaclKey(password, salt);
  const encoder = new TextEncoder();
  const { encrypted, nonce } = naclEncrypt(encoder.encode(mnemonic), secret);
  const packed = new Uint8Array(salt.length + nonce.length + encrypted.length);
  packed.set(salt, 0);
  packed.set(nonce, salt.length);
  packed.set(encrypted, salt.length + nonce.length);
  return "nacl:" + u8aToB64(packed);
}

async function decryptMnemonicFallback(encrypted: string, password: string): Promise<string> {
  const { naclDecrypt } = await import("@polkadot/util-crypto");
  const raw = encrypted.startsWith("nacl:") ? encrypted.slice(5) : encrypted;
  const packed = Uint8Array.from(atob(raw), (c) => c.charCodeAt(0));
  const salt = packed.slice(0, 16);
  const nonce = packed.slice(16, 40);
  const ciphertext = packed.slice(40);
  const secret = await deriveNaclKey(password, salt);
  const plaintext = naclDecrypt(ciphertext, nonce, secret);
  if (!plaintext) throw new Error("Decryption failed");
  return new TextDecoder().decode(plaintext);
}

async function encryptMnemonic(mnemonic: string, password: string): Promise<string> {
  if (isWebCryptoAvailable()) return encryptMnemonicWebCrypto(mnemonic, password);
  return encryptMnemonicFallback(mnemonic, password);
}

async function decryptMnemonic(encrypted: string, password: string): Promise<string> {
  if (encrypted.startsWith("nacl:")) return decryptMnemonicFallback(encrypted, password);
  if (isWebCryptoAvailable()) return decryptMnemonicWebCrypto(encrypted, password);
  throw new Error(
    "Cannot decrypt mnemonic: Web Crypto unavailable in insecure contexts; use https:// or localhost.",
  );
}

function getKeyring(): Keyring {
  return new Keyring({ type: "sr25519", ss58Format: NEX_SS58 });
}

export function buildSigner(pair: KeyringPair, getApi: ApiFactory): Signer {
  let id = 0;
  return {
    signPayload: async (payload: SignerPayloadJSON): Promise<SignerResult> => {
      const api = await getApi();
      const extrinsicPayload = api.registry.createType("ExtrinsicPayload", payload, {
        version: payload.version as unknown as number,
      });
      const { signature } = extrinsicPayload.sign(pair);
      return { id: ++id, signature: signature as `0x${string}` };
    },
    signRaw: async (raw: SignerPayloadRaw): Promise<SignerResult> => {
      const { u8aToHex, hexToU8a } = await import("@polkadot/util");
      const message = hexToU8a(raw.data);
      const signature = u8aToHex(pair.sign(message));
      return { id: ++id, signature };
    },
  };
}

export async function createAccount(
  name: string,
  password: string,
): Promise<{ mnemonic: string; address: string }> {
  assertLocalStorage();
  await cryptoWaitReady();
  const mnemonic = mnemonicGenerate();
  const keyring = getKeyring();
  const pair = keyring.addFromMnemonic(mnemonic, { name });
  const json = pair.toJson(password);
  lsWriteAccount(pair.address, json);
  const encrypted = await encryptMnemonic(mnemonic, password);
  lsWriteMnemonic(pair.address, encrypted);
  return { mnemonic, address: pair.address };
}

export async function importAccount(
  mnemonic: string,
  name: string,
  password: string,
): Promise<{ address: string }> {
  assertLocalStorage();
  const cleaned = mnemonic.trim().replace(/\s+/g, " ").toLowerCase();
  const { mnemonicValidate } = await import("@polkadot/util-crypto");
  await cryptoWaitReady();
  if (!mnemonicValidate(cleaned)) throw new Error("Invalid mnemonic phrase");
  const keyring = getKeyring();
  const pair = keyring.addFromMnemonic(cleaned, { name });
  const json = pair.toJson(password);
  lsWriteAccount(pair.address, json);
  const encrypted = await encryptMnemonic(cleaned, password);
  lsWriteMnemonic(pair.address, encrypted);
  return { address: pair.address };
}

export async function listAccounts(): Promise<DesktopAccount[]> {
  const accounts: DesktopAccount[] = [];
  for (const { content } of lsListAccounts()) {
    try {
      const json = JSON.parse(content);
      accounts.push({
        address: json.address ?? "Unknown",
        name: json.meta?.name ?? "Unknown",
        encoded: content,
      });
    } catch {
      /* skip corrupt */
    }
  }
  return accounts;
}

export async function unlockAccount(
  address: string,
  password: string,
  getApi: ApiFactory,
): Promise<{ pair: KeyringPair; signer: Signer }> {
  await cryptoWaitReady();
  const stored = lsReadAccount(address);
  if (!stored) throw new Error(`Account ${address} not found`);
  const json = JSON.parse(stored);
  const keyring = getKeyring();
  const pair = keyring.addFromJson(json);
  pair.decodePkcs8(password);
  return { pair, signer: buildSigner(pair, getApi) };
}

export async function deleteAccount(address: string): Promise<void> {
  lsDeleteAccount(address);
  lsDeleteMnemonic(address);
}

/// EN: Reveal backup mnemonic (requires account password). Returns null if no backup stored.
/// CN: 导出备份助记词（需账户密码）；无备份时返回 null。
export async function exportMnemonic(address: string, password: string): Promise<string | null> {
  const stored = lsReadMnemonic(address);
  if (!stored) return null;
  try {
    const { encrypted } = JSON.parse(stored) as { encrypted: string };
    return await decryptMnemonic(encrypted, password);
  } catch {
    throw new Error("Failed to decrypt mnemonic — wrong password?");
  }
}
