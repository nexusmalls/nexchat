// EN: pinner-hot — hot-tier multi-location pinning daemon (ADR CHAT_SYNC_ANCHOR §5.8),
// drop-in for scripts/relay-pinner.mjs. Each tick re-derives the desired pointer set,
// rotates N generations per slot (relay-core::plan_pin_ops), pins new CIDs and unpins CIDs
// no kept generation references — onto a REMOTE IPFS node (never the relay host).
// CN: pinner-hot——热层多点 pin 守护进程（ADR CHAT_SYNC_ANCHOR §5.8），drop-in 替换
// scripts/relay-pinner.mjs。每 tick 重新推导期望集合，每槽位轮转 N 代
// （relay-core::plan_pin_ops），pin 新 CID、unpin 不再被保留代次引用的 CID——目标是异机
// IPFS 节点（绝不是 relay 同机）。
//
// Env: RELAY_DATA_DIR, PINNER_IPFS_API (required), PINNER_INTERVAL_MS (30000),
//      PINNER_MAX_BLOB_BYTES (10485760), PINNER_KEEP_GENERATIONS (2), PINNER_STATE_FILE,
//      PINNER_UNPIN (1 = unpin unreferenced, default on).

use std::collections::HashSet;
use std::time::Duration;

use relay_core::{
    plan_pin_ops, read_state_file, write_state_file, PinnerState, DEFAULT_KEEP_GENERATIONS,
};
use relay_pinners::ipfs::Ipfs;
use relay_pinners::{
    data_dir, env_str, env_u64, read_desired, shutdown_signal, state_path, PinResult,
};
use tokio::time::interval;

struct Hot {
    dir: String,
    ipfs: Ipfs,
    state_file: std::path::PathBuf,
    max_blob: u64,
    keep: usize,
    unpin: bool,
    // EN: oversized CIDs skipped until restart (parity with JS `skipped` set). CN: 超限 CID 跳过至重启。
    skipped: HashSet<String>,
}

impl Hot {
    async fn tick(&mut self) -> PinResult<String> {
        let desired = read_desired(&self.dir);
        let prev: Option<PinnerState> = read_state_file(&self.state_file);
        let plan = plan_pin_ops(&desired, prev.as_ref(), self.keep);

        let mut pinned_ok: HashSet<String> = prev
            .as_ref()
            .map(|p| p.pinned.iter().cloned().collect())
            .unwrap_or_default();

        for cid in &plan.to_pin {
            if self.skipped.contains(cid) {
                continue;
            }
            match self.ipfs.size(cid).await {
                Ok(size) => {
                    if size > self.max_blob {
                        eprintln!(
                            "[relay-pinner] skip oversized cid={cid} size={size} max={}",
                            self.max_blob
                        );
                        self.skipped.insert(cid.clone());
                        continue;
                    }
                    match self.ipfs.pin_add(cid).await {
                        Ok(()) => {
                            pinned_ok.insert(cid.clone());
                            println!("[relay-pinner] pinned cid={cid} size={size}");
                        }
                        Err(e) => eprintln!("[relay-pinner] pin failed cid={cid}: {e}"),
                    }
                }
                Err(e) => eprintln!("[relay-pinner] pin failed cid={cid}: {e}"),
            }
        }

        if self.unpin {
            for cid in &plan.to_unpin {
                match self.ipfs.pin_rm(cid).await {
                    Ok(()) => {
                        pinned_ok.remove(cid);
                        println!("[relay-pinner] unpinned cid={cid}");
                    }
                    Err(e) => {
                        pinned_ok.remove(cid);
                        eprintln!("[relay-pinner] unpin failed (already gone?) cid={cid}: {e}");
                    }
                }
            }
        } else {
            for cid in &plan.to_unpin {
                pinned_ok.remove(cid);
            }
        }

        // EN: persist only CIDs actually pinned; wanted-but-failed stay out so next tick retries.
        // CN: 只持久化实际 pin 成功的 CID；想要但失败的不入库，下个 tick 自动重试。
        let mut next = plan.next_state;
        next.pinned.retain(|cid| pinned_ok.contains(cid));
        write_state_file(&self.state_file, &next)?;

        Ok(format!(
            "desired={} pinned={} +{} -{}",
            desired.len(),
            pinned_ok.len(),
            plan.to_pin.len(),
            plan.to_unpin.len()
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir();
    let api = env_str("PINNER_IPFS_API")
        .ok_or("PINNER_IPFS_API is required (remote IPFS node, NOT the relay host)")?;
    let interval_ms = env_u64("PINNER_INTERVAL_MS", 30_000);
    let keep = env_u64("PINNER_KEEP_GENERATIONS", DEFAULT_KEEP_GENERATIONS as u64) as usize;
    // EN: PINNER_UNPIN defaults to "1" (on); only an explicit "0" disables it. CN: 默认开启。
    let unpin = env_str("PINNER_UNPIN").map(|v| v == "1").unwrap_or(true);

    let mut pinner = Hot {
        ipfs: Ipfs::new(&api),
        state_file: state_path(&dir, "relay-pinner-state.json", "PINNER_STATE_FILE"),
        max_blob: env_u64("PINNER_MAX_BLOB_BYTES", 10 * 1024 * 1024),
        keep,
        unpin,
        skipped: HashSet::new(),
        dir: dir.clone(),
    };

    println!("[relay-pinner] watching {dir} → {api} every {interval_ms}ms (keep {keep} gens)");
    let mut ticker = interval(Duration::from_millis(interval_ms));
    loop {
        tokio::select! {
            _ = shutdown_signal() => break,
            _ = ticker.tick() => match pinner.tick().await {
                Ok(s) => println!("[relay-pinner] tick ok {s}"),
                Err(e) => eprintln!("[relay-pinner] tick failed: {e}"),
            },
        }
    }
    Ok(())
}
