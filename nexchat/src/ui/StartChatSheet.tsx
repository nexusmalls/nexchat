// EN: WeChat-style action sheet from the chat list 「+」 button.
// CN: 会话列表「+」弹出的微信风格动作面板。

import { useAppStore } from "@/state/appStore";

export function StartChatSheet({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const setNewDmOpen = useAppStore((s) => s.setNewDmOpen);
  const setNewGroupOpen = useAppStore((s) => s.setNewGroupOpen);
  const setJoinGroupOpen = useAppStore((s) => s.setJoinGroupOpen);

  if (!open) return null;

  return (
    <div className="dm-overlay tg-modal-overlay" onClick={onClose}>
      <aside className="wx-action-sheet" onClick={(e) => e.stopPropagation()}>
        <button
          type="button"
          className="wx-action-row"
          onClick={() => {
            onClose();
            setJoinGroupOpen(true);
          }}
        >
          <span className="wx-action-icon">🔍</span>
          <span className="wx-action-label">加入群聊</span>
        </button>
        <button
          type="button"
          className="wx-action-row"
          onClick={() => {
            onClose();
            setNewGroupOpen(true);
          }}
        >
          <span className="wx-action-icon">👥</span>
          <span className="wx-action-label">发起群聊</span>
        </button>
        <button
          type="button"
          className="wx-action-row"
          onClick={() => {
            onClose();
            setNewDmOpen(true);
          }}
        >
          <span className="wx-action-icon">💬</span>
          <span className="wx-action-label">新建聊天</span>
        </button>
        <button type="button" className="wx-action-cancel" onClick={onClose}>
          取消
        </button>
      </aside>
    </div>
  );
}
