// EN: RFC 9474 blind delivery token types (CHAT_OFFCHAIN_DELIVERY_DESIGN.md).
// CN: RFC 9474 盲签投递令牌类型。

/** EN: Token carried on a relay frame for inbox admission. CN: relay 帧上的信箱准入令牌。 */
export interface DeliveryAdmission {
  inboxId: string;
  ipkN: string;
  ipkE: string;
  epoch: number;
  ct: string;
  t: string;
  s: string;
  /** EN: RSABSSA prepared message (Randomized suite; relay verifies without re-prepare). */
  p: string;
  /** EN: Canonical pairwise MLS key for sealed-sender unseal on the receiver. CN: 接收方解封用的规范 1:1 MLS 键。 */
  mlsKey?: string;
  sealedSender?: string;
}

export interface InboxRecord {
  inboxId: string;
  epoch: number;
  saltB64: string;
  privateKeyJwk: JsonWebKey;
  publicKeyJwk: JsonWebKey;
  contactTags: Record<string, string>;
  revokedTags: string[];
}

export interface StoredToken {
  inboxId: string;
  ipkN: string;
  ipkE: string;
  epoch: number;
  ct: string;
  t: string;
  s: string;
  p: string;
  peer: string;
}

export interface TokenWalletState {
  byPeer: Record<string, StoredToken[]>;
}
