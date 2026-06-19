// EN: Minimal IPFS HTTP API client (Kubo `/api/v0/*`) used by all three pinners for size
// stat + pin add/rm. Mirrors the JS `ipfsPost` / `cidSize`: POST-only, per-call timeouts,
// non-2xx -> error with a truncated body. CN: 极简 IPFS HTTP API 客户端（Kubo `/api/v0/*`），
// 三个 pinner 共用以做 size stat 与 pin add/rm。对齐 JS 的 `ipfsPost`/`cidSize`：仅 POST、
// 单次调用超时、非 2xx 即报错并截断响应体。

use crate::PinResult;
use serde_json::Value;
use std::time::Duration;

/// EN: IPFS API client bound to one base URL (the REMOTE node, never the relay host).
/// CN: 绑定单个 base URL 的 IPFS API 客户端（异机节点，绝不是 relay 同机）。
pub struct Ipfs {
    base: String,
    client: reqwest::Client,
}

impl Ipfs {
    /// EN: Trim a trailing slash so `{base}{route}` joins cleanly. CN: 去掉末尾斜杠。
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    async fn post(&self, route: &str, timeout: Duration) -> PinResult<String> {
        let res = self
            .client
            .post(format!("{}{}", self.base, route))
            .timeout(timeout)
            .send()
            .await?;
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            let snippet: String = text.chars().take(200).collect();
            return Err(format!("{route} → {}: {snippet}", status.as_u16()).into());
        }
        Ok(text)
    }

    /// EN: Cumulative DAG size of `/ipfs/{cid}` in bytes (0 if absent). CN: CID 的累计字节大小。
    pub async fn size(&self, cid: &str) -> PinResult<u64> {
        let text = self
            .post(
                &format!("/api/v0/files/stat?arg=/ipfs/{cid}"),
                Duration::from_secs(30),
            )
            .await?;
        let v: Value = serde_json::from_str(&text)?;
        let n = v
            .get("CumulativeSize")
            .and_then(Value::as_u64)
            .or_else(|| v.get("Size").and_then(Value::as_u64))
            .unwrap_or(0);
        Ok(n)
    }

    /// EN: Recursively pin a CID on the remote node. CN: 在远端节点递归 pin 一个 CID。
    pub async fn pin_add(&self, cid: &str) -> PinResult<()> {
        self.post(
            &format!("/api/v0/pin/add?arg={cid}&recursive=true"),
            Duration::from_secs(60),
        )
        .await?;
        Ok(())
    }

    /// EN: Unpin a CID on the remote node. CN: 在远端节点解除 pin。
    pub async fn pin_rm(&self, cid: &str) -> PinResult<()> {
        self.post(
            &format!("/api/v0/pin/rm?arg={cid}"),
            Duration::from_secs(60),
        )
        .await?;
        Ok(())
    }
}
