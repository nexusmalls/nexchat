// EN: RFC 9474 RSABSSA wrapper (@cloudflare/blindrsa-ts, SHA384-PSS-Randomized).
// CN: RFC 9474 RSABSSA 封装。

import { RSABSSA } from "@cloudflare/blindrsa-ts";
import { buildTokenMessage } from "@/delivery/tokenMessage";

export const MODULUS_BITS = 3072;
const EXP = Uint8Array.from([0x01, 0x00, 0x01]);

export function deliverySuite() {
  return RSABSSA.SHA384.PSS.Randomized();
}

export async function generateInboxKeyPair(modulusLength = MODULUS_BITS) {
  const suite = deliverySuite();
  return suite.generateKey({ publicExponent: EXP, modulusLength });
}

export async function importPublicKey(jwk: JsonWebKey): Promise<CryptoKey> {
  return crypto.subtle.importKey(
    "jwk",
    jwk,
    { name: "RSA-PSS", hash: "SHA-384" },
    true,
    ["verify"],
  );
}

export async function importPrivateKey(jwk: JsonWebKey): Promise<CryptoKey> {
  // EN: extractable required — blindrsa-ts reads RSA params from the key on blindSign.
  // CN: 须可导出——blindSign 时 blindrsa-ts 要从密钥读取 RSA 参数。
  return crypto.subtle.importKey(
    "jwk",
    jwk,
    { name: "RSA-PSS", hash: "SHA-384" },
    true,
    ["sign"],
  );
}

export async function exportKeyJwk(key: CryptoKey): Promise<JsonWebKey> {
  return crypto.subtle.exportKey("jwk", key);
}

export async function blindTokenRequest(
  publicKey: CryptoKey,
  t: Uint8Array,
  ct: Uint8Array,
  epoch: number,
) {
  const suite = deliverySuite();
  const msg = buildTokenMessage(t, ct, epoch);
  const prepared = suite.prepare(msg);
  const { blindedMsg, inv } = await suite.blind(publicKey, prepared);
  return { blindedMsg, inv, preparedMsg: prepared };
}

export async function blindSignToken(privateKey: CryptoKey, blindedMsg: Uint8Array) {
  const suite = deliverySuite();
  return suite.blindSign(privateKey, blindedMsg);
}

export async function finalizeToken(
  publicKey: CryptoKey,
  preparedMsg: Uint8Array,
  blindSig: Uint8Array,
  inv: Uint8Array,
) {
  const suite = deliverySuite();
  return suite.finalize(publicKey, preparedMsg, blindSig, inv);
}

export async function verifyPrepared(
  publicKey: CryptoKey,
  signature: Uint8Array,
  preparedMsg: Uint8Array,
): Promise<boolean> {
  const suite = deliverySuite();
  return suite.verify(publicKey, signature, preparedMsg);
}
