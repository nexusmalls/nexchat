// EN: pinner-crust — disaster-tier daily pin ordering (ADR CHAT_SYNC_ANCHOR §5.8), drop-in
// for scripts/relay-crust-pinner.mjs. Once per day it scans the relay's plaintext pointer
// set and places a pin order for every newly-referenced CID through a SELF-HOSTED W3Auth
// Pinning Service (IPFS Remote Pinning API → Crust storage orders). Only-additive
// (relay-core::plan_chain_pin_requests) — a relay wipe never cancels existing orders.
// PRIVACY/CADENCE red lines per §5.8: operator-seed token; daily cadence, changed CIDs only.
// CN: pinner-crust——灾备底每日下单（ADR CHAT_SYNC_ANCHOR §5.8），drop-in 替换
// scripts/relay-crust-pinner.mjs。每日扫描 relay 明文指针集合，对新引用的 CID 经**自托管
// W3Auth Pinning Service**（IPFS Remote Pinning API → Crust 存储单）下单。只增不减
// （relay-core::plan_chain_pin_requests）——relay 清库不取消已有订单。隐私/节奏红线见 §5.8：
// 运营者 seed token；每日节奏、仅对变化的 CID 下单。
//
// Env: RELAY_DATA_DIR, CRUST_PIN_ENDPOINT (required), CRUST_PIN_TOKEN (required),
//      CRUST_PINNER_IPFS_API (or PINNER_IPFS_API, required), CRUST_PINNER_INTERVAL_MS
//      (86400000), CRUST_PINNER_MAX_BLOB_BYTES (10485760), CRUST_PINNER_MAX_PER_TICK (200),
//      CRUST_PINNER_STATE_FILE.

use std::collections::HashSet;
use std::time::Duration;

use relay_core::{
    now_ms, plan_chain_pin_requests, read_state_file, write_state_file, OnlyAddState,
};
use relay_pinners::ipfs::Ipfs;
use relay_pinners::{
    data_dir, env_str, env_u64, read_desired, shutdown_signal, state_path, PinResult,
};
use serde_json::json;
use tokio::time::interval;

struct Crust {
    dir: String,
    ipfs: Ipfs,
    client: reqwest::Client,
    endpoint: String,
    token: String,
    state_file: std::path::PathBuf,
    max_blob: u64,
    max_per_tick: usize,
    skipped: HashSet<String>,
}

impl Crust {
    /// EN: POST a pin order to the W3Auth PSA `/pins` route, returning its request id.
    /// CN: 向 W3Auth PSA `/pins` 下单，返回 request id。
    async fn submit(&self, cid: &str) -> PinResult<String> {
        let name = format!("nexchat-sync-{}", cid.chars().take(12).collect::<String>());
        let res = self
            .client
            .post(format!("{}/pins", self.endpoint))
            .header("authorization", format!("Bearer {}", self.token))
            .header("content-type", "application/json")
            .timeout(Duration::from_secs(60))
            .body(json!({ "cid": cid, "name": name }).to_string())
            .send()
            .await?;
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            let snippet: String = text.chars().take(200).collect();
            return Err(format!(
                "POST {}/pins → {}: {snippet}",
                self.endpoint,
                status.as_u16()
            )
            .into());
        }
        if text.is_empty() {
            return Ok("ok".into());
        }
        let v: serde_json::Value = serde_json::from_str(&text)?;
        Ok(v.get("requestid")
            .and_then(serde_json::Value::as_str)
            .or_else(|| v.get("requestId").and_then(serde_json::Value::as_str))
            .unwrap_or("ok")
            .to_string())
    }

    async fn tick(&mut self) -> PinResult<String> {
        let desired = read_desired(&self.dir);
        let prev: Option<OnlyAddState> = read_state_file(&self.state_file);
        // EN: same only-additive planner as the persistent tier — order each CID exactly once.
        // CN: 与持久层同一只增不减规划器——每个 CID 只下一次单。
        let plan = plan_chain_pin_requests(&desired, prev.as_ref());
        let mut next = plan.next_state;

        let mut ordered = 0u32;
        for cid in plan.to_request.iter().take(self.max_per_tick) {
            if self.skipped.contains(cid) {
                continue;
            }
            match self.ipfs.size(cid).await {
                Ok(size) => {
                    if size == 0 || size > self.max_blob {
                        eprintln!(
                            "[relay-crust-pinner] skip cid={cid} size={size} (cap {})",
                            self.max_blob
                        );
                        self.skipped.insert(cid.clone());
                        continue;
                    }
                    match self.submit(cid).await {
                        Ok(request_id) => {
                            next.requested.insert(
                                cid.clone(),
                                json!({ "at": now_ms(), "size": size, "requestId": request_id }),
                            );
                            ordered += 1;
                            println!("[relay-crust-pinner] order placed cid={cid} size={size} → {request_id}");
                        }
                        Err(e) => eprintln!("[relay-crust-pinner] order failed cid={cid} (will retry next tick): {e}"),
                    }
                }
                Err(e) => eprintln!(
                    "[relay-crust-pinner] order failed cid={cid} (will retry next tick): {e}"
                ),
            }
        }

        write_state_file(&self.state_file, &next)?;
        Ok(format!(
            "desired={} pending={} ordered={ordered}",
            desired.len(),
            plan.to_request.len()
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir();
    let endpoint = env_str("CRUST_PIN_ENDPOINT")
        .ok_or("CRUST_PIN_ENDPOINT is required (self-hosted W3Auth pinning service)")?
        .trim_end_matches('/')
        .to_string();
    let token = env_str("CRUST_PIN_TOKEN")
        .ok_or("CRUST_PIN_TOKEN is required (operator-seed auth token, NEVER a user key)")?;
    let ipfs_api = env_str("CRUST_PINNER_IPFS_API")
        .or_else(|| env_str("PINNER_IPFS_API"))
        .ok_or("CRUST_PINNER_IPFS_API (or PINNER_IPFS_API) is required for size checks")?;
    let interval_ms = env_u64("CRUST_PINNER_INTERVAL_MS", 24 * 60 * 60_000);

    let mut pinner = Crust {
        ipfs: Ipfs::new(&ipfs_api),
        client: reqwest::Client::new(),
        endpoint: endpoint.clone(),
        token,
        state_file: state_path(
            &dir,
            "relay-crust-pinner-state.json",
            "CRUST_PINNER_STATE_FILE",
        ),
        max_blob: env_u64("CRUST_PINNER_MAX_BLOB_BYTES", 10 * 1024 * 1024),
        max_per_tick: env_u64("CRUST_PINNER_MAX_PER_TICK", 200) as usize,
        skipped: HashSet::new(),
        dir: dir.clone(),
    };

    println!("[relay-crust-pinner] scanning {dir} → {endpoint} every {interval_ms}ms (daily snapshot cadence)");
    let mut ticker = interval(Duration::from_millis(interval_ms));
    loop {
        tokio::select! {
            _ = shutdown_signal() => break,
            _ = ticker.tick() => match pinner.tick().await {
                Ok(s) => println!("[relay-crust-pinner] tick ok {s}"),
                Err(e) => eprintln!("[relay-crust-pinner] tick failed: {e}"),
            },
        }
    }
    Ok(())
}
