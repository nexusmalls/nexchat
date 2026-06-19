// EN: Pending group invites in the contacts sidebar.
// CN: 联系人侧栏中的待处理群邀请。

import { useMemo } from "react";
import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { Avatar } from "@/ui/Avatar";
import { shortNexAddress } from "@/wallet/address";

export function GroupInvitesSection() {
  const groupInvites = useAppStore((s) => s.groupInvites);
  const selectedId = useUiStore((s) => s.selectedGroupInviteId);
  const selectGroupInvite = useUiStore((s) => s.selectGroupInvite);

  const pending = useMemo(
    () => [...groupInvites].sort((a, b) => b.sentAt - a.sentAt),
    [groupInvites],
  );

  if (pending.length === 0) return null;

  return (
    <section className="tg-contact-requests tg-group-invites">
      <div className="tg-contact-letter">群邀请</div>
      {pending.map((inv) => {
        const inviter = inv.fromLabel || shortNexAddress(inv.fromAddr);
        return (
          <button
            key={inv.inviteId}
            type="button"
            className={`tg-chat-row tg-contact-row tg-contact-req-row tg-group-invite-row${
              selectedId === inv.inviteId ? " active" : ""
            }`}
            onClick={() => selectGroupInvite(inv.inviteId)}
          >
            <Avatar kind="group" title={inv.groupName} />
            <div className="tg-chat-row-body">
              <div className="tg-chat-row-top">
                <span className="tg-chat-name">{inv.groupName}</span>
                <span className="tg-group-invite-badge">邀请</span>
              </div>
              <div className="tg-chat-row-bottom">
                <span className="tg-chat-preview">{inviter} 邀请你加入</span>
              </div>
            </div>
          </button>
        );
      })}
    </section>
  );
}
