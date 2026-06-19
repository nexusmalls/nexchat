// EN: Group invite detail — enter chat, retry MLS sync, or dismiss.
// CN: 群邀请详情——进入群聊、重试 MLS 同步或忽略。

import { useMemo, useState } from "react";
import { config } from "@/config";
import { openMlsEngine } from "@/mls/openMlsEngine";
import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { Avatar } from "@/ui/Avatar";
import { nexDisplayAddress, shortNexAddress } from "@/wallet/address";

function formatWhen(sentAt: number): string {
  if (!sentAt) return "";
  try {
    return new Date(sentAt).toLocaleString("zh-CN", {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

export function GroupInviteDetail({ inviteId }: { inviteId: string }) {
  const groupInvites = useAppStore((s) => s.groupInvites);
  const mlsSyncRev = useAppStore((s) => s.mlsSyncRev);
  const acceptGroupInvite = useAppStore((s) => s.acceptGroupInvite);
  const syncGroupInvite = useAppStore((s) => s.syncGroupInvite);
  const dismissGroupInvite = useAppStore((s) => s.dismissGroupInvite);
  const selectGroupInvite = useUiStore((s) => s.selectGroupInvite);
  const setMainTab = useUiStore((s) => s.setMainTab);

  const invite = useMemo(
    () => groupInvites.find((r) => r.inviteId === inviteId),
    [groupInvites, inviteId],
  );

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<string | null>(null);

  if (!invite) {
    return (
      <main className="tg-main tg-main-empty">
        <div className="tg-empty-state">
          <p>邀请不存在或已忽略</p>
          <button type="button" className="wallet-secondary" onClick={() => selectGroupInvite(null)}>
            返回
          </button>
        </div>
      </main>
    );
  }

  const inviter = invite.fromLabel || shortNexAddress(invite.fromAddr);
  const mlsReady =
    !config.useMock &&
    config.mlsBackend === "openmls" &&
    mlsSyncRev >= 0 &&
    openMlsEngine.hasGroup(`g:${invite.groupId}`);
  const when = formatWhen(invite.sentAt);

  return (
    <main className="tg-main tg-contact-detail tg-group-invite-detail">
      <header className="tg-sub-head">
        <button type="button" className="tg-sub-back" onClick={() => selectGroupInvite(null)}>
          ← 群邀请
        </button>
        <span>待加入</span>
      </header>

      <div className="tg-contact-hero">
        <Avatar kind="group" title={invite.groupName} className="tg-contact-avatar" />
        <h2>{invite.groupName}</h2>
        <p className="tg-contact-handle">{inviter} 邀请你加入群聊</p>
        <p className="tg-contact-addr">群 ID {invite.groupId}</p>
        {when && <p className="tg-contact-addr-sm">收到于 {when}</p>}
        <div className={`tg-contact-mls${mlsReady ? " ok" : ""}`}>
          {mlsReady ? "🔒 群加密已就绪，可以发消息" : "⏳ 正在建立 OpenMLS 群加密…"}
        </div>
      </div>

      <div className="tg-contact-request-actions">
        {error && <p className="wallet-error">{error}</p>}
        {progress && busy && <p className="wx-group-progress">{progress}</p>}
        <p className="tg-settings-note">
          你已在链上群成员列表中。请保持 NexChat 解锁以完成 KeyPackage 发布与 MLS 握手。
        </p>
        <button
          type="button"
          className="wallet-primary"
          disabled={busy}
          onClick={() =>
            void (async () => {
              setBusy(true);
              setError(null);
              setProgress("正在进入群聊…");
              try {
                await acceptGroupInvite(inviteId);
                setMainTab("chats");
              } catch (e) {
                setError(e instanceof Error ? e.message : String(e));
              } finally {
                setBusy(false);
                setProgress(null);
              }
            })()
          }
        >
          {busy ? "处理中…" : "进入群聊"}
        </button>
        <button
          type="button"
          className="tg-contact-msg-btn secondary"
          disabled={busy}
          onClick={() =>
            void (async () => {
              setBusy(true);
              setError(null);
              setProgress("正在同步群加密…");
              try {
                await syncGroupInvite(inviteId);
              } catch (e) {
                setError(e instanceof Error ? e.message : String(e));
              } finally {
                setBusy(false);
                setProgress(null);
              }
            })()
          }
        >
          重新同步加密
        </button>
        <button
          type="button"
          className="tg-contact-remove-btn"
          disabled={busy}
          onClick={() => {
            dismissGroupInvite(inviteId);
            selectGroupInvite(null);
          }}
        >
          忽略
        </button>
        <div className="tg-profile-row">
          <span className="tg-profile-row-label">邀请人</span>
          <span className="tg-profile-row-value">{nexDisplayAddress(invite.fromAddr)}</span>
        </div>
      </div>
    </main>
  );
}
