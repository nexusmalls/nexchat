// EN: Banner on chat list when private-group join requests are pending.
// CN: 私群入群申请待处理时，会话列表顶部提示条。

import { useAppStore } from "@/state/appStore";

export function PendingJoinBanner() {
  const pendingJoins = useAppStore((s) => s.pendingJoins);
  const setJoinGroupOpen = useAppStore((s) => s.setJoinGroupOpen);
  const openJoinPreview = useAppStore((s) => s.openJoinPreview);

  if (pendingJoins.length === 0) return null;

  const first = pendingJoins[0]!;

  return (
    <button
      type="button"
      className="tg-group-invite-banner tg-pending-join-banner"
      onClick={() => {
        void openJoinPreview(first.groupId);
        setJoinGroupOpen(true);
      }}
    >
      <span className="tg-group-invite-banner-icon" aria-hidden>
        🕐
      </span>
      <span className="tg-group-invite-banner-text">
        {pendingJoins.length === 1
          ? `「${first.title}」${statusText(first.status)}`
          : `你有 ${pendingJoins.length} 个入群申请进行中`}
      </span>
      <span className="tg-group-invite-banner-arrow" aria-hidden>
        ›
      </span>
    </button>
  );
}

function statusText(status: "pending" | "approved" | "joining"): string {
  switch (status) {
    case "pending":
      return "等待管理员批准";
    case "approved":
      return "已批准，待入群";
    case "joining":
      return "正在入群…";
    default:
      return "申请中";
  }
}
