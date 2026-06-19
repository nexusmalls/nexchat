import { useMemo, useState } from "react";
import { formatChatDisplayName } from "@/chat/displayName";
import { useContactRoster } from "@/hooks/useContactRoster";
import { canForwardMessage } from "@/p3/forward";
import { useAppStore } from "@/state/appStore";
import { Avatar } from "@/ui/Avatar";
import { contentPreviewFromMessage } from "@/ui/messagePreview";

// EN: Pick one or more conversations and forward a message (optional comment).
// CN: 选择一个或多个会话转发消息（可附言）。
export function ForwardMessageModal() {
  const roster = useContactRoster();
  const source = useAppStore((s) => s.forwardSource);
  const account = useAppStore((s) => s.account);
  const conversations = useAppStore((s) => s.conversations);
  const closeForwardPicker = useAppStore((s) => s.closeForwardPicker);
  const forwardToConversations = useAppStore((s) => s.forwardToConversations);

  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [comment, setComment] = useState("");
  const [sending, setSending] = useState(false);

  const displayNameOpts = useMemo(
    () => ({
      selfNickname: account?.nickname,
      selfAddress: account?.account,
    }),
    [account?.nickname, account?.account],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = conversations.filter((c) => !c.archived && !c.frozen);
    if (!q) return list;
    return list.filter(
      (c) =>
        c.title.toLowerCase().includes(q) ||
        (c.lastMessagePreview ?? "").toLowerCase().includes(q),
    );
  }, [conversations, query]);

  if (!source || !canForwardMessage(source)) return null;

  const senderLabel = formatChatDisplayName(source.senderRef, roster, displayNameOpts);
  const preview = contentPreviewFromMessage(source);

  const toggle = (convId: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(convId)) next.delete(convId);
      else next.add(convId);
      return next;
    });
  };

  const handleSend = async () => {
    if (selected.size === 0 || sending) return;
    setSending(true);
    try {
      await forwardToConversations([...selected], comment);
      setSelected(new Set());
      setComment("");
      setQuery("");
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="wx-share-modal-backdrop" role="presentation" onClick={closeForwardPicker}>
      <div
        className="wx-share-modal wx-forward-modal"
        role="dialog"
        aria-label="转发消息"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="wx-share-modal-head">
          <span>转发消息</span>
          <button type="button" className="wx-share-modal-close" onClick={closeForwardPicker}>
            ✕
          </button>
        </header>

        <div className="wx-forward-preview">
          <span className="wx-forward-preview-label">{senderLabel}</span>
          <span className="wx-forward-preview-body">{preview}</span>
        </div>

        <input
          className="wx-forward-search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索会话"
        />

        <div className="wx-share-modal-list wx-forward-list">
          {filtered.length === 0 ? (
            <p className="wx-market-empty">{query ? "无匹配会话" : "暂无可用会话"}</p>
          ) : (
            filtered.map((c) => {
              const checked = selected.has(c.convId);
              const disabled = c.adminMuted;
              return (
                <button
                  key={c.convId}
                  type="button"
                  className={`wx-share-modal-row wx-forward-row${checked ? " selected" : ""}${disabled ? " disabled" : ""}`}
                  disabled={disabled || sending}
                  onClick={() => toggle(c.convId)}
                >
                  <span className={`wx-forward-check${checked ? " on" : ""}`} aria-hidden>
                    {checked ? "✓" : ""}
                  </span>
                  <Avatar kind={c.kind} title={c.title} avatarCid={c.avatarCid} />
                  <span className="wx-share-modal-title">{c.title}</span>
                  {disabled && <span className="wx-forward-muted-tag">禁言</span>}
                </button>
              );
            })
          )}
        </div>

        <footer className="wx-forward-footer">
          <input
            className="wx-forward-comment"
            value={comment}
            onChange={(e) => setComment(e.target.value)}
            placeholder="给朋友留言"
            disabled={sending}
          />
          <button
            type="button"
            className="wx-forward-send"
            disabled={selected.size === 0 || sending}
            onClick={() => void handleSend()}
          >
            {sending ? "发送中…" : `发送${selected.size > 0 ? `(${selected.size})` : ""}`}
          </button>
        </footer>
      </div>
    </div>
  );
}
