# NexChat wallet model / 钱包模型

EN: NexChat uses a **built-in software wallet** (encrypted local keystore + mnemonic). Browser
**extension wallets are not** the production signing path. Users create or import an account in
`WalletGate`, unlock with a password, and all chain extrinsics + E2EI Wire leaf binding use the
unlocked **keyring pair** in-process.

CN: NexChat 使用**内置软件钱包**（本地加密 keystore + 助记词）。生产环境**不**依赖浏览器扩展钱包。
用户在 `WalletGate` 创建/导入账户、密码解锁后，链上 extrinsic 与 Wire E2EI leaf 绑定均使用进程内
已解锁的 **keyring pair** 签名。

---

## Production path / 生产路径

```
WalletGate (create / import / unlock)
  → unlockDesktopSession()     wallet/session.ts
  → setSignerPair(pair)        chain/signer.ts
  → deriveVaultMasterFromPair  wallet/vaultMaster.ts
  → keyVault.init(master)      KeyVault + encrypted IndexedDB
  → signRawWithAccountKey()    E2EI device-leaf binding (Wire §3.9)
  → chainClient.signAndSend()  polkadot.js extrinsics
```

Live builds (`VITE_USE_MOCK=false`) show `WalletGate` until the user unlocks. There is **no**
Polkadot.js extension popup in the main UI flow.

---

## Config flags / 配置项

| Flag | Production (`.env.production`) | Meaning |
|------|-------------------------------|---------|
| `VITE_USE_MOCK` | `false` | Real chain + relay |
| `VITE_DEV_WALLET` | **`false`** | Hide the `//Alice` **dev shortcut** on the welcome screen — **not** “use extension wallet” |
| Built-in wallet | always | Create/import/unlock via `WalletGate` |

When `VITE_DEV_WALLET=true` (local dev only), an extra button runs `//Seed` dev keyring for quick
multi-tab chain tests. Production keeps it off so users use the real keystore.

---

## What is NOT used in production / 生产未使用

| Path | Status |
|------|--------|
| Polkadot.js extension (`@polkadot/extension-dapp`) | Legacy helper only: `chainClient.signAndSendViaExtension()` — **not** wired from UI |
| Injector signer (`signRawWithAccountKey` → null) | Only if extension path were used; **built-in pair always signs** in prod |
| `VITE_DEV_ADDRESS` read-only demo | Mock/offline only; live mode requires unlock |

---

## Security notes / 安全说明

- Mnemonic + password encrypt the keystore in **localStorage / IndexedDB** (see `wallet/desktopKeyring.ts`).
- `vault_master` is derived from the unlocked pair secret and keys all account-scoped blobs (archive,
  conv-index, contacts, EISA anchor keys).
- **Lock wallet** clears the in-memory pair and reloads to `WalletGate`.
- Android Capacitor builds load the same web app; wallet storage is the WebView local store.

---

## Related / 相关

- UI: `src/ui/WalletGate.tsx`, `src/hooks/useWallet.ts`
- Signer: `src/chain/signer.ts`, `src/wallet/session.ts`
- Wire E2EI: `src/mls/deviceLeafCredential.ts`, `CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md` §3.9
- Intentional dual paths (Track A vs Wire): [`INTENTIONAL_DUAL_PATHS.md`](INTENTIONAL_DUAL_PATHS.md)
