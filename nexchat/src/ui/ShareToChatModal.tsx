import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { encodeProductShare } from "@/shop/shareCard";

// EN: Pick a conversation and send a product share card message.
// CN: 选择会话并发送商品分享卡片消息。
export function ShareToChatModal() {
  const draft = useUiStore((s) => s.shareProductDraft);
  const closeShareProductPicker = useUiStore((s) => s.closeShareProductPicker);
  const setMainTab = useUiStore((s) => s.setMainTab);
  const conversations = useAppStore((s) => s.conversations);
  const openConversation = useAppStore((s) => s.openConversation);
  const sendMessage = useAppStore((s) => s.sendMessage);

  if (!draft) return null;

  const handlePick = async (convId: string) => {
    const text = encodeProductShare(draft.productId, draft.shopId, draft.label);
    closeShareProductPicker();
    setMainTab("chats");
    await openConversation(convId);
    await sendMessage(text);
  };

  return (
    <div className="wx-share-modal-backdrop" role="presentation" onClick={closeShareProductPicker}>
      <div
        className="wx-share-modal"
        role="dialog"
        aria-label="分享到聊天"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="wx-share-modal-head">
          <span>分享到聊天</span>
          <button type="button" className="wx-share-modal-close" onClick={closeShareProductPicker}>
            ✕
          </button>
        </header>
        <p className="wx-share-modal-hint">
          {draft.label.trim() || `商品 #${draft.productId}`}
        </p>
        <div className="wx-share-modal-list">
          {conversations.length === 0 ? (
            <p className="wx-market-empty">暂无会话，请先发起聊天</p>
          ) : (
            conversations.map((c) => (
              <button
                key={c.convId}
                type="button"
                className="wx-share-modal-row"
                onClick={() => void handlePick(c.convId)}
              >
                <span className="wx-share-modal-title">{c.title}</span>
                <span className="wx-share-modal-preview">
                  {c.lastMessagePreview ?? c.convId}
                </span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
