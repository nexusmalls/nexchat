// EN: Banner in group chat when admin has pending join requests.
// CN: 群聊内管理员待批入群申请提示条。

import { useAppStore } from "@/state/appStore";

export function AdminJoinRequestBanner({ groupId }: { groupId: number }) {
  const count = useAppStore((s) => s.groupJoinRequestCounts[groupId] ?? 0);
  const openGroupManage = useAppStore((s) => s.openGroupManage);
  const conv = useAppStore((s) => s.conversations.find((c) => c.groupId === groupId));

  if (count === 0 || !conv) return null;
  if (conv.myRole !== "owner" && conv.myRole !== "admin") return null;

  return (
    <button
      type="button"
      className="tg-group-invite-banner tg-admin-join-banner"
      onClick={() =>
        openGroupManage({
          groupId,
          title: conv.title,
          memberCount: conv.memberCount,
          myRole: conv.myRole,
          initialTab: "joinRequests",
        })
      }
    >
      <span className="tg-group-invite-banner-icon" aria-hidden>
        📋
      </span>
      <span className="tg-group-invite-banner-text">
        {count === 1 ? "有 1 个入群申请待处理" : `有 ${count} 个入群申请待处理`}
      </span>
      <span className="tg-group-invite-banner-arrow" aria-hidden>
        ›
      </span>
    </button>
  );
}
