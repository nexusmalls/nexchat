// EN: relay-pinners common layer (ADR CHAT_SYNC_ANCHOR §5.8). Shared by the hot / chain /
// crust pinner bins: dual-track desired-pointer derivation (snapshot + journal, immune to
// journal truncation), an IPFS HTTP client for size stat + pin ops, env helpers, and the
// SIGINT/SIGTERM shutdown future. The pure planners + state-file IO live in relay-core; the
// daemons here add the network IO and the periodic tick loop around them.
// CN: relay-pinners 公共层（ADR CHAT_SYNC_ANCHOR §5.8）。被 hot / chain / crust 三个 bin 共用：
// 双轨期望指针推导（快照 + journal，免疫 journal 截断）、用于 size stat 与 pin 操作的 IPFS
// HTTP 客户端、环境变量辅助函数，以及 SIGINT/SIGTERM 关停 future。纯规划器与 state 文件 IO 在
// relay-core；此处守护进程在其外层加网络 IO 与周期 tick 循环。

pub mod ipfs;

use relay_core::{collect_desired_pointers, Pointer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// EN: Pinner IO result (network + parse errors are boxed). CN: pinner IO 结果。
pub type PinResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// EN: Non-empty env var, else None (parity with JS `process.env.X ?? default`).
/// CN: 取非空环境变量，否则 None。
pub fn env_str(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// EN: Parse a numeric env var, falling back to `default`. CN: 解析数值环境变量。
pub fn env_u64(key: &str, default: u64) -> u64 {
    env_str(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// EN: Relay data dir (`RELAY_DATA_DIR`, else `$CWD/data`). CN: relay 数据目录。
pub fn data_dir() -> String {
    env_str("RELAY_DATA_DIR").unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_default()
            .join("data")
            .to_string_lossy()
            .into_owned()
    })
}

/// EN: Resolve a bookkeeping state file path (`$ENV` override else `$dir/$default_name`).
/// CN: 解析记账 state 文件路径。
pub fn state_path(dir: &str, default_name: &str, env_key: &str) -> PathBuf {
    env_str(env_key)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(dir).join(default_name))
}

/// EN: Re-derive the full desired slot->pointer set from the relay snapshot + journal every
/// tick (dual-track consumption, §5.8). Missing/corrupt files yield an empty input, never an
/// error — same tolerance as the JS pinners. CN: 每 tick 由快照 + journal 全量重新推导期望
/// 槽位→指针集合（§5.8 双轨消费）；文件缺失/损坏视为空输入，绝不报错。
pub fn read_desired(dir: &str) -> BTreeMap<String, Pointer> {
    let snapshot: Option<Value> = std::fs::read_to_string(Path::new(dir).join("relay-state.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let journal =
        std::fs::read_to_string(Path::new(dir).join("relay-journal.ndjson")).unwrap_or_default();
    collect_desired_pointers(snapshot.as_ref(), &journal)
}

/// EN: Resolve on SIGINT/SIGTERM (unix) or Ctrl-C (other). CN: SIGINT/SIGTERM 或 Ctrl-C 后返回。
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
        let mut int = signal(SignalKind::interrupt()).expect("SIGINT handler");
        tokio::select! {
            _ = term.recv() => {},
            _ = int.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
