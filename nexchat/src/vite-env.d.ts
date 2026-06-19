/// <reference types="vite/client" />

declare const __NEXCHAT_APP_VERSION__: string;
declare const __NEXCHAT_APP_BUILT_AT__: string;

interface ImportMetaEnv {
  readonly VITE_WS_ENDPOINT: string;
  readonly VITE_HTTP_ENDPOINT: string;
  readonly VITE_RELAY_WS: string;
  readonly VITE_DELIVERY_TOKEN_BATCH: string;
  readonly VITE_USE_MOCK: string;
  readonly VITE_MLS_ENABLED: string;
  readonly VITE_APP_NAME: string;
  readonly VITE_DEV_ADDRESS: string;
  readonly VITE_MLS_BACKEND: string;
  readonly VITE_MLS_DEMO_GROUP: string;
  readonly VITE_MLS_CONTROL_PLANE: string;
  readonly VITE_DEV_SEED: string;
  readonly VITE_MLS_ROSTER: string;
  readonly VITE_DEFAULT_CONTACTS: string;
  readonly VITE_IPFS_ENABLED: string;
  readonly VITE_IPFS_API_URL: string;
  readonly VITE_IPFS_GATEWAY_URL: string;
  readonly VITE_IPFS_CHUNK_THRESHOLD: string;
  readonly VITE_IPFS_CHUNK_SIZE: string;
  readonly VITE_IPFS_THUMB_MAX_PX: string;
  readonly VITE_IPFS_PIN_ENABLED: string;
  readonly VITE_IPFS_MEDIA_LOCAL_PIN: string;
  readonly VITE_IPFS_MEDIA_LOCAL_PIN_TTL_MS: string;
  readonly VITE_CONV_INDEX_ENABLED: string;
  readonly VITE_DELIVERY_TOKENS_ENABLED: string;
  readonly VITE_DELIVERY_RSA_MODULUS: string;
  readonly VITE_DEV_WALLET: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
