// EN: relay-server bin — tokio WebSocket relay (production NexChat relay). One writer task per
// connection (mpsc-fed sink); a single mutex guards all shared state; a debounced task writes
// snapshots; SIGINT/SIGTERM flush then exit.
// CN: relay-server 可执行——tokio WebSocket relay（NexChat 生产 relay）。每连接一个 writer 任务
// （mpsc 喂 sink）；单 mutex 守护共享态；防抖任务写快照；SIGINT/SIGTERM 先 flush 再退出。

mod config;
mod mailbox;
mod protocol;
mod state;
mod token;

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use config::Config;
use protocol::{process, Conn};
use state::{Server, Tx};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::from_env();
    let port = cfg.port;
    let rate_limit = cfg.rate_limit;
    let max_msg_bytes = cfg.max_msg_bytes;
    let server = Arc::new(Server::load(cfg)?);

    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    println!("[nexchat-relay] listening ws://127.0.0.1:{port}");

    spawn_flusher(server.clone());
    spawn_shutdown(server.clone());

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let server = server.clone();
        let is_loopback = peer.ip().is_loopback();
        tokio::spawn(async move {
            let _ = handle_conn(server, stream, is_loopback, rate_limit, max_msg_bytes).await;
        });
    }
}

/// EN: Debounced snapshot writer (300ms after the first dirty mark). CN: 防抖快照写入器。
fn spawn_flusher(server: Arc<Server>) {
    tokio::spawn(async move {
        loop {
            server.flush_notify.notified().await;
            tokio::time::sleep(Duration::from_millis(300)).await;
            let mut inner = server.inner.lock().unwrap_or_else(|e| e.into_inner());
            if inner.dirty {
                server.flush(&mut inner);
            }
        }
    });
}

/// EN: On SIGINT/SIGTERM, flush durable state then exit. CN: SIGINT/SIGTERM 时 flush 后退出。
fn spawn_shutdown(server: Arc<Server>) {
    tokio::spawn(async move {
        shutdown_signal().await;
        {
            let mut inner = server.inner.lock().unwrap_or_else(|e| e.into_inner());
            server.flush(&mut inner);
        }
        std::process::exit(0);
    });
}

async fn handle_conn(
    server: Arc<Server>,
    stream: TcpStream,
    is_loopback: bool,
    rate_limit: u32,
    max_msg_bytes: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut write, mut read) = ws.split();
    let (tx, mut rx): (Tx, mpsc::UnboundedReceiver<Message>) = mpsc::unbounded_channel();

    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if write.send(m).await.is_err() {
                break;
            }
        }
    });

    let mut conn = Conn {
        id: None,
        account: None,
        is_loopback,
    };
    // Per-connection sliding 1-minute rate window (relay-limits.mjs).
    let mut bucket_count: u32 = 0;
    let mut bucket_reset = relay_core::now_ms() + 60_000;

    while let Some(item) = read.next().await {
        let frame = match item {
            Ok(f) => f,
            Err(_) => break,
        };
        let text = match frame {
            Message::Text(t) => t.as_str().to_owned(),
            Message::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
            Message::Ping(p) => {
                let _ = tx.send(Message::Pong(p));
                continue;
            }
            Message::Close(_) => break,
            _ => continue,
        };
        let now = relay_core::now_ms();
        if now >= bucket_reset {
            bucket_count = 0;
            bucket_reset = now + 60_000;
        }
        // EN: count first so oversize frames also consume rate budget (bounds the reject-parse
        // cost below). CN: 先计数，让超大帧也占用频率额度（同时限制下方 reject 解析成本）。
        bucket_count += 1;
        let over_rate = bucket_count > rate_limit;
        // EN: oversize frames are dropped without parsing here; the client pre-checks frame size
        // and surfaces the failure locally with the exact message context. CN: 超大帧在此不解析
        // 直接丢弃；客户端会在发送前自检大小并就地标记对应消息失败。
        if text.len() > max_msg_bytes {
            continue;
        }
        // EN: rate-limited frames get a best-effort NACK so the sender doesn't silently lose them.
        // CN: 被限流的帧回一条尽力 NACK，避免发送方静默丢消息。
        if over_rate {
            protocol::reject_frame(&tx, &text, "rate_limited");
            continue;
        }
        process(&server, &mut conn, &tx, &text);
    }

    {
        let mut inner = server.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(id) = &conn.id {
            inner.clients.remove(id);
            if let Some(account) = &conn.account {
                if let Some(set) = inner.endpoints_by_account.get_mut(account) {
                    set.remove(id);
                    if set.is_empty() {
                        inner.endpoints_by_account.remove(account);
                    }
                }
            }
        }
    }
    drop(tx);
    let _ = writer.await;
    Ok(())
}

async fn shutdown_signal() {
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
