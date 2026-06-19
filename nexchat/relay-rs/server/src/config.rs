// EN: Relay server runtime config from environment (TTL caps, strict auth, mailbox limits).
// CN: relay server 运行时配置（环境变量：TTL、严格鉴权、邮箱上限等）。

use std::env;

const MIN: u64 = 60 * 1000;
const HOUR: u64 = 60 * MIN;
const DAY: u64 = 24 * HOUR;

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key).ok().as_deref() {
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES") => true,
        Some("0") | Some("false") | Some("FALSE") | Some("no") | Some("NO") => false,
        Some(_) => default,
        None => default,
    }
}

/// EN: Parsed relay configuration. CN: 解析后的 relay 配置。
#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub max_msg_bytes: usize,
    pub rate_limit: u32,
    pub admin_secret: String,
    pub data_dir: String,
    pub spent_cap: usize,
    pub chat_ttl_ms: u64,
    pub chat_max_frames: usize,
    pub chat_max_bytes: u64,
    /// EN: When true: require sr25519 `account_sig` on register_account, auth-gated fetches/consumes,
    /// and reject (not fallback) failed sealed delivery. CN: 为 true 时：`register_account` 须
    /// sr25519 `account_sig`；fetch/consume 须会话鉴权；sealed delivery 验签失败直接 reject。
    pub strict_auth: bool,
    pub mls_max_frames: usize,
    pub mls_max_bytes: u64,
    pub contact_max_entries: usize,
    pub debug: bool,
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = env::var("RELAY_DATA_DIR").unwrap_or_else(|_| {
            let cwd = env::current_dir().unwrap_or_default();
            cwd.join("data").to_string_lossy().to_string()
        });
        Config {
            port: env_u64("RELAY_PORT", 8765) as u16,
            max_msg_bytes: env_u64("RELAY_MAX_MSG_BYTES", 256 * 1024) as usize,
            rate_limit: env_u64("RELAY_RATE_LIMIT", 120) as u32,
            admin_secret: env::var("RELAY_ADMIN_SECRET").unwrap_or_default(),
            data_dir,
            spent_cap: env_u64("RELAY_SPENT_CAP", 50_000) as usize,
            chat_ttl_ms: env_u64("RELAY_CHAT_MAILBOX_TTL_MS", 180 * DAY),
            chat_max_frames: env_u64("RELAY_CHAT_MAILBOX_MAX_FRAMES", 5000) as usize,
            chat_max_bytes: env_u64("RELAY_CHAT_MAILBOX_MAX_BYTES", 256 * 1024 * 1024),
            strict_auth: env_bool("RELAY_STRICT_AUTH", false),
            mls_max_frames: env_u64("RELAY_MLS_MAILBOX_MAX_FRAMES", 2000) as usize,
            mls_max_bytes: env_u64("RELAY_MLS_MAILBOX_MAX_BYTES", 64 * 1024 * 1024),
            contact_max_entries: env_u64("RELAY_CONTACT_MAILBOX_MAX_ENTRIES", 1000) as usize,
            debug: env::var("RELAY_DEBUG").map(|v| v == "1").unwrap_or(false),
        }
    }
}

/// EN/CN: TTLs that are not env-tunable (fixed defaults).
pub const MLS_CTRL_TTL_MS: u64 = 7 * DAY;
pub const CONTACT_TTL_MS: u64 = 30 * DAY;
pub const GROUP_INVITE_TTL_MS: u64 = 7 * DAY;
