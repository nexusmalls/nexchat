// EN: pinner-chain — persistent-tier pinning daemon (ADR CHAT_SYNC_ANCHOR §5.8), drop-in for
// scripts/relay-chain-pinner.mjs. Periodically scans the relay's plaintext pointer set and
// requests an on-chain `pallet-storage-service` Standard pin for every CID not yet covered
// (only-additive, relay-core::plan_chain_pin_requests). PRIVACY RED LINE (§5.8): the signer
// MUST be the OPERATOR account — never a user account, or the "AccountId → plaintext CID"
// mapping leaks on-chain through the billing path. Trigger is the periodic relay scan, NOT
// chain AnchorPublished events (anchors are ciphertext). Requires the `chain` feature.
// CN: pinner-chain——持久层 pin 守护进程（ADR CHAT_SYNC_ANCHOR §5.8），drop-in 替换
// scripts/relay-chain-pinner.mjs。周期扫描 relay 明文指针集合，对未覆盖的 CID 发起链上
// `pallet-storage-service` Standard pin（只增不减，relay-core::plan_chain_pin_requests）。
// 隐私红线（§5.8）：签名者**必须是运营者账户**——绝不可用用户账户，否则「AccountId → 明文
// CID」映射会经计费路径回流上链。触发为周期扫描 relay，而非链上 AnchorPublished 事件（锚是
// 密文）。需启用 `chain` feature。
//
// Env: RELAY_DATA_DIR, CHAIN_PINNER_WS (ws://127.0.0.1:9944),
//      CHAIN_PINNER_OPERATOR_SURI (required), CHAIN_PINNER_SUBJECT_ID (required),
//      CHAIN_PINNER_IPFS_API (or PINNER_IPFS_API, required), CHAIN_PINNER_INTERVAL_MS
//      (1800000), CHAIN_PINNER_MAX_BLOB_BYTES (10485760), CHAIN_PINNER_MAX_PER_TICK (50),
//      CHAIN_PINNER_STATE_FILE.

use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;

use relay_core::{
    now_ms, plan_chain_pin_requests, read_state_file, write_state_file, OnlyAddState,
};
use relay_pinners::ipfs::Ipfs;
use relay_pinners::{
    data_dir, env_str, env_u64, read_desired, shutdown_signal, state_path, PinResult,
};
use serde_json::json;
use subxt::dynamic::Value;
use subxt::{OnlineClient, SubstrateConfig};
use subxt_signer::sr25519::Keypair;
use subxt_signer::SecretUri;
use tokio::time::interval;

/// EN: Lazily-established chain connection (api + operator signer). CN: 懒建立的链连接。
struct ChainConn {
    api: OnlineClient<SubstrateConfig>,
    signer: Keypair,
}

struct Chain {
    dir: String,
    ipfs: Ipfs,
    ws_url: String,
    operator_suri: String,
    subject_id: u64,
    state_file: std::path::PathBuf,
    max_blob: u64,
    max_per_tick: usize,
    skipped: HashSet<String>,
    conn: Option<ChainConn>,
}

impl Chain {
    /// EN: Connect on first use (matches the JS lazy connect). CN: 首次使用时连接。
    async fn ensure_conn(&mut self) -> PinResult<&ChainConn> {
        if self.conn.is_none() {
            let api = OnlineClient::<SubstrateConfig>::from_url(&self.ws_url).await?;
            let signer = Keypair::from_uri(&SecretUri::from_str(&self.operator_suri)?)?;
            self.conn = Some(ChainConn { api, signer });
        }
        Ok(self.conn.as_ref().unwrap())
    }

    /// EN: Submit `StorageService::request_pin_for_subject(subject_id, cid, size, Some(Standard))`
    /// via a dynamic extrinsic (no compile-time metadata). `AlreadyPinned` counts as success.
    /// CN: 通过动态 extrinsic 提交（无需编译期 metadata）。`AlreadyPinned` 视为成功。
    async fn submit(&mut self, cid: &str, size: u64) -> PinResult<String> {
        let subject_id = self.subject_id;
        let conn = self.ensure_conn().await?;
        let call = subxt::dynamic::tx(
            "StorageService",
            "request_pin_for_subject",
            vec![
                Value::u128(subject_id as u128),
                Value::from_bytes(cid.as_bytes()),
                Value::u128(size as u128),
                Value::unnamed_variant("Some", vec![Value::unnamed_variant("Standard", vec![])]),
            ],
        );
        let in_block = conn
            .api
            .tx()
            .sign_and_submit_then_watch_default(&call, &conn.signer)
            .await?
            .wait_for_finalized()
            .await?;
        let hash = in_block.extrinsic_hash();
        match in_block.wait_for_success().await {
            Ok(_) => Ok(format!("{hash:?}")),
            // EN: AlreadyPinned = another path pinned it first — success for us. CN: 已 pin 即成功。
            Err(e) if e.to_string().contains("AlreadyPinned") => Ok("already-pinned".into()),
            Err(e) => Err(e.into()),
        }
    }

    async fn tick(&mut self) -> PinResult<String> {
        let desired = read_desired(&self.dir);
        let prev: Option<OnlyAddState> = read_state_file(&self.state_file);
        let plan = plan_chain_pin_requests(&desired, prev.as_ref());
        let mut next = plan.next_state;

        let mut requested = 0u32;
        let to_do: Vec<String> = plan
            .to_request
            .iter()
            .take(self.max_per_tick)
            .cloned()
            .collect();
        for cid in &to_do {
            if self.skipped.contains(cid) {
                continue;
            }
            match self.ipfs.size(cid).await {
                Ok(size) => {
                    if size == 0 || size > self.max_blob {
                        eprintln!(
                            "[relay-chain-pinner] skip cid={cid} size={size} (cap {})",
                            self.max_blob
                        );
                        self.skipped.insert(cid.clone());
                        continue;
                    }
                    match self.submit(cid, size).await {
                        Ok(outcome) => {
                            next.requested
                                .insert(cid.clone(), json!({ "at": now_ms(), "size": size }));
                            requested += 1;
                            println!("[relay-chain-pinner] pin requested cid={cid} size={size} → {outcome}");
                        }
                        Err(e) => eprintln!("[relay-chain-pinner] pin request failed cid={cid} (will retry next tick): {e}"),
                    }
                }
                Err(e) => eprintln!(
                    "[relay-chain-pinner] pin request failed cid={cid} (will retry next tick): {e}"
                ),
            }
        }

        write_state_file(&self.state_file, &next)?;
        Ok(format!(
            "desired={} pending={} requested={requested}",
            desired.len(),
            plan.to_request.len()
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir();
    let ws_url = env_str("CHAIN_PINNER_WS").unwrap_or_else(|| "ws://127.0.0.1:9944".into());
    let operator_suri = env_str("CHAIN_PINNER_OPERATOR_SURI")
        .ok_or("CHAIN_PINNER_OPERATOR_SURI is required (operator account, NEVER a user account)")?;
    let subject_id: u64 = env_str("CHAIN_PINNER_SUBJECT_ID")
        .and_then(|v| v.parse().ok())
        .ok_or("CHAIN_PINNER_SUBJECT_ID is required (numeric subject id owned by the operator)")?;
    let ipfs_api = env_str("CHAIN_PINNER_IPFS_API")
        .or_else(|| env_str("PINNER_IPFS_API"))
        .ok_or("CHAIN_PINNER_IPFS_API (or PINNER_IPFS_API) is required for size checks")?;
    let interval_ms = env_u64("CHAIN_PINNER_INTERVAL_MS", 30 * 60_000);

    let mut pinner = Chain {
        ipfs: Ipfs::new(&ipfs_api),
        ws_url: ws_url.clone(),
        operator_suri,
        subject_id,
        state_file: state_path(
            &dir,
            "relay-chain-pinner-state.json",
            "CHAIN_PINNER_STATE_FILE",
        ),
        max_blob: env_u64("CHAIN_PINNER_MAX_BLOB_BYTES", 10 * 1024 * 1024),
        max_per_tick: env_u64("CHAIN_PINNER_MAX_PER_TICK", 50) as usize,
        skipped: HashSet::new(),
        conn: None,
        dir: dir.clone(),
    };

    println!("[relay-chain-pinner] scanning {dir} → {ws_url} (subject {subject_id}, Standard tier) every {interval_ms}ms");
    let mut ticker = interval(Duration::from_millis(interval_ms));
    loop {
        tokio::select! {
            _ = shutdown_signal() => break,
            _ = ticker.tick() => match pinner.tick().await {
                Ok(s) => println!("[relay-chain-pinner] tick ok {s}"),
                Err(e) => eprintln!("[relay-chain-pinner] tick failed: {e}"),
            },
        }
    }
    Ok(())
}
