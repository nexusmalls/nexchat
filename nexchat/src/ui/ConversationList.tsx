import { useMemo, useRef, useState, type ReactNode } from "react";
import { StartChatSheet } from "@/ui/StartChatSheet";
import { useAppStore } from "@/state/appStore";
import { config } from "@/config";
import { useWallet } from "@/hooks/useWallet";
import type { ConversationVM } from "@/types/viewModels";
import { Avatar } from "@/ui/Avatar";
import { WeChatNavBar } from "@/ui/WeChatNavBar";
import { formatChatTime } from "@/ui/formatTime";
import { GroupInviteBanner } from "@/ui/GroupInviteBanner";
import { PendingJoinBanner } from "@/ui/PendingJoinBanner";

// EN: Telegram-style chat list — search bar, avatar rows, unread pills.
// CN: Telegram 风格会话列表——搜索栏、头像行、未读角标。
export function ConversationList() {
  const { conversations, activeConvId, openConversation } = useAppStore();
  const { lock } = useWallet();
  const [query, setQuery] = useState("");
  const [startSheetOpen, setStartSheetOpen] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return conversations;
    return conversations.filter(
      (c) =>
        c.title.toLowerCase().includes(q) ||
        (c.lastMessagePreview ?? "").toLowerCase().includes(q),
    );
  }, [conversations, query]);

  const actions: ReactNode = (
    <>
      <button
        type="button"
        className="wx-nav-icon-btn"
        onClick={() => searchRef.current?.focus()}
        title="搜索"
      >
        <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden>
          <circle cx="11" cy="11" r="6.5" stroke="currentColor" strokeWidth="1.8" fill="none" />
          <path d="M16 16l4 4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
        </svg>
      </button>
      <button
        type="button"
        className="wx-nav-icon-btn wx-nav-plus"
        onClick={() => setStartSheetOpen(true)}
        title="发起聊天或群聊"
      >
        ⊕
      </button>
      {!config.useMock && (
        <button type="button" className="wx-nav-icon-btn" onClick={lock} title="锁定钱包">
          🔒
        </button>
      )}
    </>
  );

  return (
    <aside className="tg-sidebar wx-panel">
      <StartChatSheet open={startSheetOpen} onClose={() => setStartSheetOpen(false)} />
      <WeChatNavBar title={config.appName} actions={actions} />

      <div className="tg-search-wrap wx-search-wrap">
        <span className="tg-search-icon" aria-hidden>
          🔍
        </span>
        <input
          ref={searchRef}
          className="tg-search wx-search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索"
        />
      </div>

      <div className="tg-chat-list">
        <GroupInviteBanner />
        <PendingJoinBanner />
        {filtered.map((c) => (
          <ConvRow
            key={c.convId}
            conv={c}
            active={c.convId === activeConvId}
            onClick={() => void openConversation(c.convId)}
          />
        ))}
        {filtered.length === 0 && (
          <div className="tg-list-empty">{query ? "无匹配会话" : "暂无会话"}</div>
        )}
      </div>
    </aside>
  );
}

function ConvRow({
  conv,
  active,
  onClick,
}: {
  conv: ConversationVM;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button className={`tg-chat-row${active ? " active" : ""}`} onClick={onClick} type="button">
      <Avatar kind={conv.kind} title={conv.title} avatarCid={conv.avatarCid} />
      <div className="tg-chat-row-body">
        <div className="tg-chat-row-top">
          <span className="tg-chat-name">
            {conv.pinned && <span className="tg-pin">📌</span>}
            {conv.title}
          </span>
          <span className="tg-chat-time">{formatChatTime(conv.recency)}</span>
        </div>
        <div className="tg-chat-row-bottom">
          <span className="tg-chat-preview">
            {conv.dnd && <span className="tg-flag">🔕</span>}
            {conv.adminMuted && <span className="tg-flag">🚫</span>}
            {conv.frozen && <span className="tg-flag">❄️</span>}
            {conv.lastMessagePreview ?? (conv.kind === "group" ? `${conv.memberCount} 名成员` : "开始聊天")}
          </span>
          <span className="tg-chat-badges">
            {(conv.mentionUnread ?? 0) > 0 && <span className="tg-mention">@</span>}
            {conv.unread > 0 && <span className="tg-unread-count">{conv.unread}</span>}
          </span>
        </div>
      </div>
    </button>
  );
}
