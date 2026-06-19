# nexchat-dr (`dr-wasm/`)

EN: vodozemac (Olm) wrapper compiled to WASM for the decentralized 1:1 stack (X3DH + Double Ratchet).
This crate is the **only** place 1:1 cryptography happens and is **strictly decoupled** from `mls-wasm`.

CN: vodozemac（Olm）封装，编译为 WASM，供去中心化 1:1 栈（X3DH + 双棘轮）使用。
本 crate 是 **1:1 密码学唯一所在**，与 `mls-wasm` **严格解耦**。

Design reference: `pallets/chat/CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md` (§17.1 Olm mapping).

TS engine: `nexchat/src/crypto-dr/vodozemacEngine.ts` (`VodozemacEngine`).

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122   # match mls-wasm
```

## Build WASM (browser + vitest)

From `nexchat/`:

```bash
npm run dr:build    # → src/dr-pkg/nexchat_dr.js + nexchat_dr_bg.wasm
```

Manual equivalent:

```bash
cd dr-wasm
CARGO_TARGET_DIR=./target cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir ../src/dr-pkg --out-name nexchat_dr \
  target/wasm32-unknown-unknown/release/nexchat_dr.wasm
```

Commit `src/dr-pkg/` after rebuilding so CI/tests run without a Rust toolchain (same policy as `mls-pkg/`).

## Native integration tests

Host-target smoke tests (no wasm32 required):

```bash
npm run test:dr
# or: cd dr-wasm && CARGO_TARGET_DIR=./target-native cargo test
```

Note: `JsError` paths are wasm-only; reject/tamper cases are covered in
`src/crypto-dr/vodozemacEngine.test.ts` on the TS side.

End-to-end TS tests (requires built WASM in `src/dr-pkg/`):

```bash
npm test -- src/crypto-dr/
```

## `DrClient` API (WASM)

| Method | Role |
|--------|------|
| `identityKey` / `ed25519Key` | Device IK (Curve25519) + Olm signing key |
| `generateOneTimeKeys` / `oneTimeKeys` / `markKeysAsPublished` | OPK lifecycle |
| `generateFallbackKey` / `fallbackKey` | SPK fallback (Olm fallback key) |
| `createOutboundSession` | X3DH initiator |
| `createInboundSession` | X3DH responder (returns first plaintext + sender IK) |
| `encrypt` / `decrypt` | Double Ratchet (`[msg_type:u8] ‖ body`) |
| `pickle` / `restore` | Account persistence (encryption is TS store's job) |
| `pickleSession` / `loadSession` | Per-peer session persistence |

Account-key endorsements (sr25519) and on-chain prekey publication are done in TS / chain — this crate performs **Olm crypto only**.

## Launch checklist (crate)

- [ ] `src/dr-pkg/` rebuilt and committed after API changes (`npm run dr:verify`)
- [ ] `npm run test:dr` green
- [ ] `npm test -- src/crypto-dr/` green
- [ ] CI: nexus `.github/workflows/nexchat-dr.yml` (monorepo) or `nexchat/.github/workflows/dr-wasm.yml` (standalone repo)