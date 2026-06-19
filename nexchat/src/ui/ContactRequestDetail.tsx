// EN: Inbound contact request — accept with display name or reject.
// CN: 入站联系人请求——填写显示名接受或拒绝。

import { useMemo, useState } from "react";
import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { peerAvatarCid, usePeerAvatarMap } from "@/hooks/usePeerAvatarMap";
import { Avatar } from "@/ui/Avatar";
import { nexDisplayAddress, shortNexAddress } from "@/wallet/address";

export function ContactRequestDetail({ reqId }: { reqId: string }) {
  const contactRequests = useAppStore((s) => s.contactRequests);
  const acceptContactRequest = useAppStore((s) => s.acceptContactRequest);
  const rejectContactRequest = useAppStore((s) => s.rejectContactRequest);
  const selectContactRequest = useUiStore((s) => s.selectContactRequest);
  const selectContact = useUiStore((s) => s.selectContact);

  const req = useMemo(
    () => contactRequests.find((r) => r.reqId === reqId),
    [contactRequests, reqId],
  );
  const avatarMap = usePeerAvatarMap(req ? [req.peerAddress] : [], !!req);

  const [label, setLabel] = useState(req?.fromLabel ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!req) {
    return (
      <main className="tg-main tg-main-empty">
        <div className="tg-empty-state">
          <p>请求不存在或已处理</p>
          <button type="button" className="wallet-secondary" onClick={() => selectContactRequest(null)}>
            返回
          </button>
        </div>
      </main>
    );
  }

  const title = req.fromLabel || shortNexAddress(req.peerAddress);
  const pending = req.status === "pending";
  const avatarCid = peerAvatarCid(avatarMap, req.peerAddress);

  return (
    <main className="tg-main tg-contact-detail tg-contact-request-detail">
      <header className="tg-sub-head">
        <button type="button" className="tg-sub-back" onClick={() => selectContactRequest(null)}>
          ← 联系人请求
        </button>
        <span>{pending ? "待处理" : req.status === "accepted" ? "已接受" : "已拒绝"}</span>
      </header>

      <div className="tg-contact-hero">
        <Avatar kind="direct" title={title} avatarCid={avatarCid} className="tg-contact-avatar" />
        <h2>{title}</h2>
        <p className="tg-contact-handle">请求添加你为联系人</p>
        <p className="tg-contact-addr">{nexDisplayAddress(req.peerAddress)}</p>
        <p className="tg-contact-addr-sm">{shortNexAddress(req.peerAddress, 8, 6)}</p>
      </div>

      {pending ? (
        <div className="tg-contact-request-actions">
          {error && <p className="wallet-error">{error}</p>}
          <label className="wallet-field">
            <span>为对方设置显示名称</span>
            <input
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={req.fromLabel || "显示名称"}
            />
          </label>
          <button
            type="button"
            className="wallet-primary"
            disabled={busy || label.trim().length < 1}
            onClick={() =>
              void (async () => {
                setBusy(true);
                setError(null);
                try {
                  await acceptContactRequest(reqId, label.trim());
                  selectContact(req.peerAddress);
                } catch (e) {
                  setError(e instanceof Error ? e.message : String(e));
                } finally {
                  setBusy(false);
                }
              })()
            }
          >
            {busy ? "处理中…" : "接受"}
          </button>
          <button
            type="button"
            className="tg-contact-remove-btn"
            disabled={busy}
            onClick={() =>
              void (async () => {
                setBusy(true);
                await rejectContactRequest(reqId);
                selectContactRequest(null);
                setBusy(false);
              })()
            }
          >
            拒绝
          </button>
        </div>
      ) : (
        <p className="tg-settings-note tg-contact-request-done">
          {req.status === "accepted"
            ? "你已接受该请求，对方会收到确认。"
            : "你已拒绝该请求。"}
        </p>
      )}
    </main>
  );
}
