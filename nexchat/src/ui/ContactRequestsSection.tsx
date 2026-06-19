// EN: Pending inbound contact requests in the contacts sidebar.
// CN: 联系人侧栏中的待处理入站请求。

import { useMemo } from "react";
import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { peerAvatarCid, usePeerAvatarMap } from "@/hooks/usePeerAvatarMap";
import { Avatar } from "@/ui/Avatar";
import { shortNexAddress } from "@/wallet/address";

export function ContactRequestsSection() {
  const contactRequests = useAppStore((s) => s.contactRequests);
  const selectedId = useUiStore((s) => s.selectedContactRequestId);
  const selectContactRequest = useUiStore((s) => s.selectContactRequest);

  const pending = useMemo(
    () =>
      contactRequests
        .filter((r) => r.direction === "inbound" && r.status === "pending")
        .sort((a, b) => b.sentAt - a.sentAt),
    [contactRequests],
  );
  const avatarMap = usePeerAvatarMap(pending.map((r) => r.peerAddress));

  if (pending.length === 0) return null;

  return (
    <section className="tg-contact-requests">
      <div className="tg-contact-letter">请求</div>
      {pending.map((r) => {
        const title = r.fromLabel || shortNexAddress(r.peerAddress);
        return (
          <button
            key={r.reqId}
            type="button"
            className={`tg-chat-row tg-contact-row tg-contact-req-row${
              selectedId === r.reqId ? " active" : ""
            }`}
            onClick={() => selectContactRequest(r.reqId)}
          >
            <Avatar kind="direct" title={title} avatarCid={peerAvatarCid(avatarMap, r.peerAddress)} />
            <div className="tg-chat-row-body">
              <div className="tg-chat-row-top">
                <span className="tg-chat-name">{title}</span>
                <span className="tg-contact-req-badge">新</span>
              </div>
              <div className="tg-chat-row-bottom">
                <span className="tg-chat-preview">请求添加你为联系人</span>
              </div>
            </div>
          </button>
        );
      })}
    </section>
  );
}
