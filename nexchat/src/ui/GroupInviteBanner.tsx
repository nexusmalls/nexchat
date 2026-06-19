// EN: Compact banner on the chats tab when group invites are pending.
// CN: 聊天 Tab 顶部的群邀请提示条。

import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";

export function GroupInviteBanner() {
  const groupInvites = useAppStore((s) => s.groupInvites);
  const setMainTab = useUiStore((s) => s.setMainTab);
  const selectGroupInvite = useUiStore((s) => s.selectGroupInvite);

  if (groupInvites.length === 0) return null;

  const first = [...groupInvites].sort((a, b) => b.sentAt - a.sentAt)[0]!;

  return (
    <button
      type="button"
      className="tg-group-invite-banner"
      onClick={() => {
        setMainTab("contacts");
        selectGroupInvite(first.inviteId);
      }}
    >
      <span className="tg-group-invite-banner-icon" aria-hidden>
        👥
      </span>
      <span className="tg-group-invite-banner-text">
        {groupInvites.length === 1
          ? `「${first.groupName}」群邀请待查看`
          : `你有 ${groupInvites.length} 个群邀请待查看`}
      </span>
      <span className="tg-group-invite-banner-arrow" aria-hidden>
        ›
      </span>
    </button>
  );
}
