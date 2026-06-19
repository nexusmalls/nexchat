import { useEffect, useMemo, useState } from "react";
import { chainClient } from "@/chain/chainClient";
import { config } from "@/config";
import { useContactRoster } from "@/hooks/useContactRoster";
import { useAppStore } from "@/state/appStore";
import { canonicalAddress } from "@/wallet/address";

// EN: Invite contacts into an existing on-chain group (owner/admin).
// CN: 向已有链上群邀请联系人（群主/管理员）。
export function InviteGroupMembers() {
  const { inviteGroupTarget, closeInviteGroupMembers, inviteGroupMembers, account } =
    useAppStore();
  const roster = useContactRoster();
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [memberCount, setMemberCount] = useState(inviteGroupTarget?.memberCount ?? 1);

  const open = inviteGroupTarget != null;
  const peers = useMemo(
    () => roster.filter((m) => m.address !== account?.account),
    [roster, account?.account],
  );

  useEffect(() => {
    if (open) {
      setPicked(new Set());
      setError(null);
      setProgress(null);
      setMemberCount(inviteGroupTarget?.memberCount ?? 1);
    }
  }, [open, inviteGroupTarget?.groupId, inviteGroupTarget?.memberCount]);

  useEffect(() => {
    if (!open || !inviteGroupTarget || config.useMock) return;
    let cancelled = false;
    void (async () => {
      try {
        const snap = await chainClient.groupSnapshot(inviteGroupTarget.groupId);
        if (!cancelled && snap) setMemberCount(snap.memberCount);
      } catch {
        /* keep conversation count */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, inviteGroupTarget]);

  if (!open || !inviteGroupTarget) return null;

  const chainOk = !config.useMock && config.mlsControlPlane === "chain";
  const pickOk = picked.size >= 1;
  const canSubmit = pickOk && !busy;

  const toggle = (addr: string) => {
    const canon = canonicalAddress(addr);
    setPicked((prev) => {
      const next = new Set(prev);
      if (next.has(canon)) next.delete(canon);
      else next.add(canon);
      return next;
    });
    setError(null);
  };

  const onInvite = async () => {
    if (!pickOk || !chainOk || busy) return;
    setBusy(true);
    setError(null);
    setProgress("正在准备邀请…");
    let queued = false;
    try {
      const result = await inviteGroupMembers([...picked], {
        onProgress: (message) => setProgress(message),
      });
      if (result === "queued") {
        queued = true;
        setPicked(new Set());
        return;
      }
      closeInviteGroupMembers();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      if (!queued) setProgress(null);
    }
  };

  return (
    <div
      className="dm-overlay tg-modal-overlay"
      onClick={() => !busy && closeInviteGroupMembers()}
    >
      <aside className="dm-panel tg-modal wx-group-modal" onClick={(e) => e.stopPropagation()}>
        <header className="dm-head">
          <span>邀请成员</span>
          <button type="button" onClick={() => !busy && closeInviteGroupMembers()} disabled={busy}>
            ✕
          </button>
        </header>

        {!chainOk ? (
          <p className="dm-hint wx-group-warn">邀请成员需要链上 MLS 控制面。</p>
        ) : (
          <>
            <p className="dm-hint">
              向「{inviteGroupTarget.title}」邀请新成员。对方需解锁 NexChat 并发布 KeyPackage。
              当前 {memberCount} 人。
              {memberCount <= 1 ? " 可每次邀请 1 人；首次加密入群需累计邀请至少 2 人。" : ""}
            </p>
            <p className="wx-group-pick-hint">已选 {picked.size} 人</p>
            <ul className="dm-list wx-group-pick-list">
              {peers.map((m) => {
                const canon = canonicalAddress(m.address);
                const on = picked.has(canon);
                return (
                  <li key={m.address}>
                    <button
                      type="button"
                      className={`wx-group-pick-row${on ? " on" : ""}`}
                      onClick={() => toggle(m.address)}
                      disabled={busy}
                    >
                      <span className={`wx-group-check${on ? " on" : ""}`}>{on ? "✓" : ""}</span>
                      <span className="dm-name">{m.labels[0] ?? m.ref}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
            {peers.length === 0 && <p className="dm-empty">通讯录为空，请先添加联系人</p>}
            {error && <p className="wallet-error wx-group-error">{error}</p>}
            {progress && busy && <p className="wx-group-progress">{progress}</p>}
            <button
              type="button"
              className="tg-add-contact-btn wx-group-create-btn"
              disabled={!canSubmit || !chainOk}
              onClick={() => void onInvite()}
            >
              {busy ? "邀请中…" : "发送邀请"}
            </button>
          </>
        )}
      </aside>
    </div>
  );
}
