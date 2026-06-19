// EN: KeyVault — the ONLY place keys live. Derives per-purpose symmetric keys from the
// account `vault_master` (ADR CHAT_SYNC_ANCHOR §5.0) via HKDF (WebCrypto). The master is
// derived from the unlocked pair's sr25519 secret (see `src/wallet/vaultMaster.ts`), so
// none of the derived keys (K_index / K_contacts / K_archive / local-store) can be computed
// from the public address. A legacy base (SHA-256 of the SS58 address — the pre-§5.0 root,
// publicly computable) is kept ONLY to auto-migrate existing ciphertexts to the new root.
// CN: KeyVault——密钥唯一所在。用账户 `vault_master`（ADR CHAT_SYNC_ANCHOR §5.0）经 HKDF
// （WebCrypto）派生各用途对称密钥。master 派生自已解锁 pair 的 sr25519 secret（见
// `src/wallet/vaultMaster.ts`），因此所有派生钥（K_index / K_contacts / K_archive / 本地库）
// 均不可由公开地址计算。旧基钥（SHA-256(SS58 地址)，§5.0 之前的根，全网可算）**仅**保留用于
// 把存量密文自动迁移到新根。
//
// ⚠️ Phase-1 placeholder note: `deriveConvKey` is still transport encryption standing in for
// the MLS application-message layer until OpenMLS WASM is fully wired; the RELAY still never
// sees plaintext. / ⚠️ Phase-1 占位说明：`deriveConvKey` 仍是 MLS 应用消息层接入前的传输加密
// 占位；relay 始终看不到明文。

const enc = new TextEncoder();

async function legacyBaseFromSeed(seed: string): Promise<CryptoKey> {
  const raw = await crypto.subtle.digest("SHA-256", enc.encode(seed));
  return crypto.subtle.importKey("raw", raw, "HKDF", false, ["deriveKey"]);
}

export class KeyVault {
  private hkdfKey: Promise<CryptoKey> | null = null;
  private legacyHkdfKey: Promise<CryptoKey> | null = null;
  private masterRoot = false;

  /// EN: Initialize the HKDF base from the 32-byte `vault_master` (required on production
  /// paths — no default seed). `legacySeed` (usually the SS58 address) enables the legacy
  /// base used by the one-time ciphertext migration. CN: 用 32 字节 `vault_master` 初始化
  /// HKDF 基钥（生产路径必传——无默认种子）。`legacySeed`（通常为 SS58 地址）启用旧基钥，
  /// 供一次性密文迁移使用。
  init(vaultMaster: Uint8Array, opts?: { legacySeed?: string }): void {
    this.hkdfKey = crypto.subtle.importKey("raw", vaultMaster as BufferSource, "HKDF", false, [
      "deriveKey",
    ]);
    this.legacyHkdfKey = opts?.legacySeed ? legacyBaseFromSeed(opts.legacySeed) : null;
    this.masterRoot = true;
  }

  /// EN: Test/mock-only init replicating the legacy root (SHA-256 of a seed string); the
  /// legacy base aliases the main base so versioned-blob fallback is a no-op. NEVER use on
  /// production paths. CN: 仅测试/mock 的初始化，复刻旧根（SHA-256(种子串)）；旧基钥与主基钥
  /// 相同，版本化 blob 回退为空操作。生产路径禁用。
  initForTest(seed: string): void {
    this.hkdfKey = legacyBaseFromSeed(seed);
    this.legacyHkdfKey = this.hkdfKey;
    this.masterRoot = false;
  }

  /// EN: Drop all key material (lock / account switch). CN: 清除全部密钥材料（锁定/切账户）。
  clear(): void {
    this.hkdfKey = null;
    this.legacyHkdfKey = null;
    this.masterRoot = false;
  }

  /// EN: True when rooted in `vault_master` (legacy→master ciphertext migration may run).
  /// CN: 根为 `vault_master` 时为 true（此时才允许执行旧根→master 的密文迁移）。
  hasMasterRoot(): boolean {
    return this.masterRoot;
  }

  private base(): Promise<CryptoKey> {
    if (!this.hkdfKey) {
      throw new Error("KeyVault not initialized (unlock wallet first / call initForTest)");
    }
    return this.hkdfKey;
  }

  private async deriveAes(
    basePromise: Promise<CryptoKey>,
    salt: string,
    info: string,
  ): Promise<CryptoKey> {
    const base = await basePromise;
    return crypto.subtle.deriveKey(
      { name: "HKDF", hash: "SHA-256", salt: enc.encode(salt), info: enc.encode(info) },
      base,
      { name: "AES-GCM", length: 256 },
      false,
      ["encrypt", "decrypt"],
    );
  }

  /// EN: Derive an AES-256-GCM key for a conversation. CN: 为会话派生 AES-256-GCM 密钥。
  async deriveConvKey(convId: string): Promise<CryptoKey> {
    return this.deriveAes(this.base(), "chat/conv-key/v1", convId);
  }

  /// EN: Derive a STABLE, non-extractable AES-256-GCM key for at-rest encryption of the
  /// local message/conversation database (per `namespace`, usually the account). It is
  /// deterministic across refreshes (so prior data decrypts) yet the key material never
  /// leaves WebCrypto. CN: 为本地消息/会话数据库的静态加密派生一个**稳定**且不可导出的
  /// AES-256-GCM 密钥（按 `namespace`，通常是账户）。跨刷新确定（旧数据可解密），但密钥材料
  /// 永不离开 WebCrypto。
  async deriveLocalStoreKey(namespace: string): Promise<CryptoKey> {
    return this.deriveAes(this.base(), "chat/local-store/v1", namespace);
  }

  /// EN: AES-256-GCM key for at-rest encryption of the decentralized 1:1 (DR) Olm pickles
  /// — the account pickle + per-peer-device session pickles. Frozen context
  /// `nexchat/x3dh/pickle/v1` (design §17.2). Deterministic across restarts (so prior
  /// ratchet state decrypts) yet never leaves WebCrypto; STRICTLY separate from the MLS
  /// key space (decoupling, design §9). CN: 去中心化 1:1（DR）Olm pickle 静态加密的
  /// AES-256-GCM 密钥——账户 pickle + 每对端设备会话 pickle。冻结上下文
  /// `nexchat/x3dh/pickle/v1`（设计 §17.2）。跨重启确定（旧棘轮态可解密）但永不离开
  /// WebCrypto；与 MLS 密钥空间严格隔离（解耦，设计 §9）。
  async deriveDrSessionKey(namespace: string): Promise<CryptoKey> {
    return this.deriveAes(this.base(), "nexchat/x3dh/pickle/v1", namespace);
  }

  /// EN: AES-256-GCM key for the encrypted cross-device conversation-index blob
  /// (`K_index = KDF(vault_master, "chat/conv-index/v1")`, CHAT_P2 §2.1).
  /// CN: 跨设备加密会话索引 blob 的 AES-256-GCM 密钥（CHAT_P2 §2.1）。
  async deriveConvIndexKey(): Promise<CryptoKey> {
    return this.deriveAes(this.base(), "chat/conv-index/v1", "v1");
  }

  /// EN: AES-256-GCM key for the encrypted cross-device contact-book vault blob
  /// (`K_contacts = KDF(vault_master, "chat/contacts-vault/v1")`).
  /// CN: 跨设备加密通讯录 vault blob 的 AES-256-GCM 密钥。
  async deriveContactsVaultKey(): Promise<CryptoKey> {
    return this.deriveAes(this.base(), "chat/contacts-vault/v1", "v1");
  }

  /// EN: AES-256-GCM key for the encrypted cross-device message-history archive blob
  /// (`K_archive = KDF(vault_master, "chat/msg-archive/v1")`).
  /// CN: 跨设备加密消息历史归档 blob 的 AES-256-GCM 密钥。
  async deriveMsgArchiveKey(): Promise<CryptoKey> {
    return this.deriveAes(this.base(), "chat/msg-archive/v1", "v1");
  }

  // EN: Legacy-root variants (pre-§5.0 base = SHA-256(address)) — migration reads only;
  // return null when no legacy seed was provided. CN: 旧根变体（§5.0 前基钥 = SHA-256(地址)）
  // ——仅供迁移读取；未提供旧种子时返回 null。

  async deriveLegacyLocalStoreKey(namespace: string): Promise<CryptoKey | null> {
    if (!this.legacyHkdfKey) return null;
    return this.deriveAes(this.legacyHkdfKey, "chat/local-store/v1", namespace);
  }

  async deriveLegacyConvIndexKey(): Promise<CryptoKey | null> {
    if (!this.legacyHkdfKey) return null;
    return this.deriveAes(this.legacyHkdfKey, "chat/conv-index/v1", "v1");
  }

  async deriveLegacyContactsVaultKey(): Promise<CryptoKey | null> {
    if (!this.legacyHkdfKey) return null;
    return this.deriveAes(this.legacyHkdfKey, "chat/contacts-vault/v1", "v1");
  }

  async deriveLegacyMsgArchiveKey(): Promise<CryptoKey | null> {
    if (!this.legacyHkdfKey) return null;
    return this.deriveAes(this.legacyHkdfKey, "chat/msg-archive/v1", "v1");
  }
}

export const keyVault = new KeyVault();
