import { useMemo, useState } from "react";
import { config } from "@/config";
import { useAppStore } from "@/state/appStore";
import { useContactRoster } from "@/hooks/useContactRoster";
import { canonicalAddress } from "@/wallet/address";
import { SigningPinRestoreButton } from "@/ui/SigningPinRestoreButton";

// EN: Create an on-chain MLS group — pick ≥2 contacts + group name.
// CN: 发起链上 MLS 群聊——选 ≥2 联系人并填写群名。
export function NewGroupChat() {
  const {
    newGroupOpen,
    setNewGroupOpen,
    account,
    createGroupChat,
    loading,
    groupSendMode,
    requestGroupSendAuthority,
  } = useAppStore();
  const roster = useContactRoster();
  const [name, setName] = useState("");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);

  const peers = useMemo(
    () => roster.filter((m) => m.address !== account?.account),
    [roster, account?.account],
  );

  if (!newGroupOpen) return null;

  const canSubmit = name.trim().length > 0 && picked.size >= 2 && !busy && !loading;
  const chainOk = !config.useMock && config.mlsControlPlane === "chain";
  // EN: Track A read-only create/send block is RETIRED on the group side under group Wire (design §10):
  // a per-device leaf can always create + send. CN: 群 Wire 下轨 A 只读建群/发送阻断在群侧**退役**（设计
  // §10）：按设备 leaf 恒可建群 + 发送。
  const trackAGroup = config.mlsVaultEnabled && !config.wireGroupMultileafEnabled;
  const readOnlySend =
    trackAGroup && groupSendMode !== "primary" && groupSendMode !== "restoring";
  const sendBlocked = trackAGroup && groupSendMode !== "primary";

  const toggle = (addr: string) => {
    const canon = canonicalAddress(addr);
    setPicked((prev) => {
      const next = new Set(prev);
      if (next.has(canon)) next.delete(canon);
      else next.add(canon);
      return next;
    });
  };

  const onCreate = async () => {
    if (!canSubmit) return;
    setBusy(true);
    try {
      await createGroupChat(name.trim(), [...picked]);
      setName("");
      setPicked(new Set());
      setNewGroupOpen(false);
    } catch {
      /* error surfaced via appStore.error */
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="dm-overlay tg-modal-overlay" onClick={() => !busy && setNewGroupOpen(false)}>
      <aside className="dm-panel tg-modal wx-group-modal" onClick={(e) => e.stopPropagation()}>
        <header className="dm-head">
          <span>发起群聊</span>
          <button type="button" onClick={() => !busy && setNewGroupOpen(false)} disabled={busy}>
            ✕
          </button>
        </header>

        {!chainOk ? (
          <p className="dm-hint wx-group-warn">
            发起群聊需要连接链上节点：请设置 <code>VITE_USE_MOCK=false</code> 且
            <code>VITE_MLS_CONTROL_PLANE=chain</code>，并启动 <code>nexus-node --dev</code>。
          </p>
        ) : (
          <>
            <p className="dm-hint">
              链上群聊端到端加密。请至少选择 2 位联系人；对方需已解锁 NexChat 以发布 KeyPackage。
            </p>
            {sendBlocked && (
              <p className="dm-hint wx-group-warn">
                {groupSendMode === "restoring"
                  ? "正在恢复本设备的发送权，请稍候…"
                  : "此设备为只读（已从云端恢复），无法建群或发言。"}
                {readOnlySend && (
                  <>
                    {" "}
                    <SigningPinRestoreButton disabled={busy} />
                    <button
                      type="button"
                      className="tg-handoff-btn"
                      disabled={busy}
                      onClick={() => void requestGroupSendAuthority()}
                    >
                      在此设备发送
                    </button>
                  </>
                )}
              </p>
            )}
            <label className="wx-group-name-field">
              <span>群名称</span>
              <input
                className="tg-profile-input wx-group-name-input"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="例如：项目讨论组"
                maxLength={32}
                disabled={busy}
              />
            </label>
            <p className="wx-group-pick-hint">
              已选 {picked.size} 人{picked.size < 2 ? "（至少 2 人）" : ""}
            </p>
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
                      <span className="dm-addr">{m.address.slice(0, 12)}…</span>
                    </button>
                  </li>
                );
              })}
            </ul>
            {peers.length === 0 && (
              <p className="dm-empty">通讯录为空，请先添加至少 2 位联系人</p>
            )}
            {!canSubmit && !busy && !loading && peers.length > 0 && (
              <p className="dm-hint wx-group-warn">
                {name.trim().length === 0
                  ? "请先填写群名称"
                  : picked.size < 2
                    ? "请至少选择 2 位联系人"
                    : ""}
              </p>
            )}
            <button
              type="button"
              className="tg-add-contact-btn wx-group-create-btn"
              disabled={!canSubmit || !chainOk || sendBlocked}
              onClick={() => void onCreate()}
            >
              {busy ? "创建中…" : "创建群聊"}
            </button>
          </>
        )}
      </aside>
    </div>
  );
}
