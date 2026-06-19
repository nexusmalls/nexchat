// EN: Background sync for relay offline chat mailbox (fetch on unlock + visibility poll).
// CN: relay 离线聊天邮箱后台同步（解锁拉取 + 页面可见性轮询）。

import { config } from "@/config";
import { fetchChatMailbox } from "@/relay/chatMailbox";
import type { RelayFrame } from "@/relay/relayClient";

const CHAT_INBOX_POLL_MS = 60_000;
const BURST_WARN_THRESHOLD = 8;

export interface ChatMailboxSyncDeps {
  selfAddress: string;
  onFrame: (frame: RelayFrame) => Promise<void>;
  /** EN: Optional gate (e.g. wait for MLS handshake) before pulling mailbox. CN: 拉信箱前可选门禁（如等 MLS 握手）。 */
  beforeSync?: () => Promise<void>;
}

export class ChatMailboxSync {
  private onVisible: (() => void) | null = null;

  constructor(private deps: ChatMailboxSyncDeps) {}

  wire(): void {
    // EN: Do not sync here — unlock() sets account after wire(); early sync drops all frames.
    // CN: 不在 wire 时同步——unlock 在 wire 之后才写入 account，过早 sync 会丢帧。
    if (!config.relayWs || typeof document === "undefined") return;

    window.setInterval(() => void this.syncInbox(), CHAT_INBOX_POLL_MS);
    this.onVisible = () => {
      if (document.visibilityState === "visible") void this.syncInbox();
    };
    document.addEventListener("visibilitychange", this.onVisible);
  }

  /// EN: Fetch relay chat mailbox and deliver each frame through the inbound pipeline.
  /// CN: 拉取 relay 聊天邮箱并逐帧走入站管线。
  async syncInbox(): Promise<number> {
    if (!config.relayWs) return 0;
    try {
      await this.deps.beforeSync?.();
      const frames = await fetchChatMailbox(this.deps.selfAddress);
      if (frames.length === 0) return 0;
      if (frames.length >= BURST_WARN_THRESHOLD) {
        console.info(`[nexchat] offline chat mailbox burst: ${frames.length} frame(s)`);
      }
      for (const frame of frames) {
        await this.deps.onFrame(frame);
      }
      return frames.length;
    } catch (e) {
      console.warn("[nexchat] chat mailbox sync failed:", e);
      return 0;
    }
  }
}
