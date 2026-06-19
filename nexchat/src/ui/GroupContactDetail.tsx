import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { Avatar } from "@/ui/Avatar";
import type { GroupRole } from "@/types/viewModels";

// EN: Joined group detail in contacts tab — open group chat CTA.
// CN: 联系人 Tab 中的已加入群详情——进入群聊。

function roleLabel(role: GroupRole): string {
  switch (role) {
    case "owner":
      return "群主";
    case "admin":
      return "管理员";
    case "member":
      return "成员";
    default:
      return "—";
  }
}

export function GroupContactDetail({ convId }: { convId: string }) {
  const conv = useAppStore((s) => s.conversations.find((c) => c.convId === convId));
  const openConversation = useAppStore((s) => s.openConversation);
  const mls = useAppStore((s) => s.mls);
  const selectGroup = useUiStore((s) => s.selectGroup);
  const setMainTab = useUiStore((s) => s.setMainTab);

  if (!conv || conv.kind !== "group") {
    return (
      <main className="tg-main tg-main-empty">
        <div className="tg-empty-state">
          <p>群聊不存在或已离开</p>
          <button type="button" className="tg-welcome-primary" onClick={() => selectGroup(null)}>
            返回列表
          </button>
        </div>
      </main>
    );
  }

  const mlsReady = mls?.ready ?? false;

  return (
    <main className="tg-main tg-contact-detail tg-group-contact-detail">
      <header className="tg-sub-head">
        <button type="button" className="tg-sub-back" onClick={() => selectGroup(null)}>
          ← 联系人
        </button>
        <span>{conv.title}</span>
      </header>

      <div className="tg-contact-hero">
        <Avatar
          kind="group"
          title={conv.title}
          avatarCid={conv.avatarCid}
          className="tg-contact-avatar"
        />
        <h2>{conv.title}</h2>
        <p className="tg-contact-handle">群聊 · ID {conv.groupId ?? "—"}</p>
        <p className="tg-contact-addr-sm">{conv.memberCount} 名成员</p>

        <div className={`tg-contact-mls${mlsReady ? " ok" : ""}`}>
          {mlsReady ? "🔒 OpenMLS 群加密已就绪" : "⏳ MLS 群握手进行中"}
          {conv.frozen && " · 群已冻结（只读）"}
        </div>

        <button
          type="button"
          className="tg-contact-msg-btn"
          onClick={() => {
            void openConversation(convId);
            setMainTab("chats");
          }}
        >
          💬 进入群聊
        </button>
        {!conv.frozen && (conv.myRole === "owner" || conv.myRole === "admin") && (
          <button
            type="button"
            className="tg-contact-msg-btn secondary"
            onClick={() =>
              useAppStore.getState().openInviteGroupMembers({
                groupId: conv.groupId ?? 0,
                title: conv.title,
                memberCount: conv.memberCount,
              })
            }
          >
            ➕ 邀请成员
          </button>
        )}
        {!conv.frozen && (
          <button
            type="button"
            className="tg-contact-msg-btn secondary"
            onClick={() =>
              useAppStore.getState().openGroupManage({
                groupId: conv.groupId ?? 0,
                title: conv.title,
                memberCount: conv.memberCount,
                myRole: conv.myRole,
              })
            }
          >
            👥 成员管理
          </button>
        )}
      </div>

      <section className="tg-profile-section">
        <h3>群信息</h3>
        <div className="tg-profile-row">
          <span className="tg-profile-row-label">我的角色</span>
          <span className="tg-profile-row-value">{roleLabel(conv.myRole)}</span>
        </div>
        <div className="tg-profile-row">
          <span className="tg-profile-row-label">成员数</span>
          <span className="tg-profile-row-value">{conv.memberCount}</span>
        </div>
        <div className="tg-profile-row">
          <span className="tg-profile-row-label">未读</span>
          <span className="tg-profile-row-value">{conv.unread > 0 ? conv.unread : "无"}</span>
        </div>
      </section>
    </main>
  );
}
