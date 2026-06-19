import { useCallback, useEffect, useState } from "react";
import { config } from "@/config";
import {
  joinStatusLabel,
  loadRecentLookups,
  saveRecentLookup,
} from "@/group/groupJoinFlow";
import type { GroupLookupVM, RecentGroupLookup } from "@/group/groupJoinTypes";
import { useAppStore } from "@/state/appStore";
import { Avatar } from "@/ui/Avatar";
import { shortNexAddress } from "@/wallet/address";

type Step = "input" | "preview";

function parseGroupId(raw: string): number | null {
  const trimmed = raw.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const id = Number(trimmed);
  if (!Number.isFinite(id) || id < 0) return null;
  return id;
}

function extractGroupIdFromPaste(text: string): number | null {
  const direct = parseGroupId(text);
  if (direct != null) return direct;
  const urlMatch = text.match(/(?:join[=/]|group[=/]|id=)(\d+)/i);
  if (urlMatch) return parseGroupId(urlMatch[1]!);
  const hashMatch = text.match(/#(\d+)/);
  if (hashMatch) return parseGroupId(hashMatch[1]!);
  return null;
}

// EN: Join a private group by numeric group_id — lookup preview + request_join.
// CN: 按群 ID 查找私群并申请加入。
export function JoinGroupById() {
  const {
    joinGroupOpen,
    setJoinGroupOpen,
    joinPreviewGroupId,
    account,
    loading,
    lookupGroupForJoin,
    requestJoinGroup,
    cancelJoinRequestGroup,
    publishKeyPackageForJoin,
    openConversation,
  } = useAppStore();

  const [step, setStep] = useState<Step>("input");
  const [groupIdInput, setGroupIdInput] = useState("");
  const [lookup, setLookup] = useState<GroupLookupVM | null>(null);
  const [recent, setRecent] = useState<RecentGroupLookup[]>([]);
  const [busy, setBusy] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const chainOk =
    !config.useMock && config.mlsControlPlane === "chain" && config.mlsBackend === "openmls";

  const reset = useCallback(() => {
    setStep("input");
    setGroupIdInput("");
    setLookup(null);
    setLocalError(null);
    setBusy(false);
  }, []);

  const runLookup = useCallback(
    async (gid: number) => {
      if (!account) return;
      setBusy(true);
      setLocalError(null);
      try {
        const vm = await lookupGroupForJoin(gid);
        setLookup(vm);
        setStep("preview");
        if (vm.exists) {
          saveRecentLookup(account.account, {
            groupId: gid,
            title: vm.name,
            lookedAt: Date.now(),
          });
          setRecent(loadRecentLookups(account.account));
        }
      } catch (e) {
        setLocalError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [account, lookupGroupForJoin],
  );

  useEffect(() => {
    if (!joinGroupOpen) {
      reset();
      return;
    }
    if (account) setRecent(loadRecentLookups(account.account));
    const fromStore = joinPreviewGroupId;
    const q =
      typeof window !== "undefined"
        ? new URLSearchParams(window.location.search).get("joinGroup")
        : null;
    const gid = fromStore ?? (q != null ? parseGroupId(q) : null);
    if (gid != null) {
      setGroupIdInput(String(gid));
      void runLookup(gid);
    }
  }, [joinGroupOpen, joinPreviewGroupId, account?.account, reset, runLookup]);

  const onFind = () => {
    const gid = extractGroupIdFromPaste(groupIdInput);
    if (gid == null) {
      setLocalError("请输入有效的群 ID（正整数）");
      return;
    }
    void runLookup(gid);
  };

  const onRequestJoin = async () => {
    if (!lookup || !account) return;
    setBusy(true);
    setLocalError(null);
    try {
      await requestJoinGroup(lookup.groupId);
      const vm = await lookupGroupForJoin(lookup.groupId);
      setLookup(vm);
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const onCancelRequest = async () => {
    if (!lookup) return;
    setBusy(true);
    setLocalError(null);
    try {
      await cancelJoinRequestGroup(lookup.groupId);
      const vm = await lookupGroupForJoin(lookup.groupId);
      setLookup(vm);
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const onPublishKeyPackage = async () => {
    setBusy(true);
    setLocalError(null);
    try {
      await publishKeyPackageForJoin();
      if (lookup) {
        const vm = await lookupGroupForJoin(lookup.groupId);
        setLookup(vm);
      }
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const onEnterGroup = async () => {
    if (!lookup) return;
    setJoinGroupOpen(false);
    await openConversation(`g:${lookup.groupId}`);
  };

  if (!joinGroupOpen) return null;

  const renderCta = () => {
    if (!lookup?.exists) return null;
    switch (lookup.status) {
      case "not_member":
        return (
          <button
            type="button"
            className="tg-add-contact-btn wx-group-create-btn"
            disabled={busy || loading || !chainOk}
            onClick={() => void onRequestJoin()}
          >
            {busy ? "提交中…" : "申请加入"}
          </button>
        );
      case "key_package_missing":
        return (
          <button
            type="button"
            className="tg-add-contact-btn wx-group-create-btn"
            disabled={busy || loading || !chainOk}
            onClick={() => void onPublishKeyPackage()}
          >
            {busy ? "发布中…" : "发布 KeyPackage 后再申请"}
          </button>
        );
      case "pending_request":
        return (
          <>
            <button type="button" className="tg-add-contact-btn wx-group-create-btn" disabled>
              等待管理员批准
            </button>
            <button
              type="button"
              className="wx-join-cancel-btn"
              disabled={busy || loading}
              onClick={() => void onCancelRequest()}
            >
              撤回申请
            </button>
          </>
        );
      case "approved_pending_welcome":
        return (
          <button type="button" className="tg-add-contact-btn wx-group-create-btn" disabled>
            已批准，等待管理员将你加入群
          </button>
        );
      case "already_member":
        return (
          <button
            type="button"
            className="tg-add-contact-btn wx-group-create-btn"
            disabled={busy}
            onClick={() => void onEnterGroup()}
          >
            进入群聊
          </button>
        );
      default:
        return (
          <button type="button" className="tg-add-contact-btn wx-group-create-btn" disabled>
            {joinStatusLabel(lookup.status)}
          </button>
        );
    }
  };

  return (
    <div
      className="dm-overlay tg-modal-overlay"
      onClick={() => !busy && setJoinGroupOpen(false)}
    >
      <aside className="dm-panel tg-modal wx-group-modal" onClick={(e) => e.stopPropagation()}>
        <header className="dm-head">
          <span>{step === "input" ? "加入群聊" : "群预览"}</span>
          <button
            type="button"
            onClick={() => !busy && setJoinGroupOpen(false)}
            disabled={busy}
          >
            ✕
          </button>
        </header>

        {!chainOk ? (
          <p className="dm-hint wx-group-warn">
            加入群聊需要连接链上节点：请设置 <code>VITE_USE_MOCK=false</code> 且
            <code>VITE_MLS_CONTROL_PLANE=chain</code>，并启动 <code>nexus-node --dev</code>。
          </p>
        ) : step === "input" ? (
          <>
            <p className="dm-hint">
              输入私群 ID 查找并申请加入。申请后需群主或管理员批准；成员关系在链上公开，消息仍端到端加密。
            </p>
            <label className="wx-group-name-field">
              <span>群 ID</span>
              <input
                className="tg-profile-input wx-group-name-input"
                value={groupIdInput}
                onChange={(e) => {
                  setGroupIdInput(e.target.value);
                  setLocalError(null);
                }}
                placeholder="例如：42"
                inputMode="numeric"
                disabled={busy}
                onKeyDown={(e) => {
                  if (e.key === "Enter") onFind();
                }}
              />
            </label>
            <button
              type="button"
              className="tg-add-contact-btn wx-group-create-btn"
              disabled={busy || !groupIdInput.trim()}
              onClick={onFind}
            >
              {busy ? "查找中…" : "查找群"}
            </button>
            {recent.length > 0 && (
              <>
                <p className="wx-group-pick-hint">最近查找</p>
                <ul className="dm-list wx-group-pick-list">
                  {recent.map((r) => (
                    <li key={r.groupId}>
                      <button
                        type="button"
                        className="wx-group-pick-row"
                        disabled={busy}
                        onClick={() => {
                          setGroupIdInput(String(r.groupId));
                          void runLookup(r.groupId);
                        }}
                      >
                        <span className="dm-name">{r.title}</span>
                        <span className="dm-addr">#{r.groupId}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </>
        ) : lookup ? (
          <>
            <div className="wx-join-preview-head">
              <Avatar kind="group" title={lookup.name} avatarCid={lookup.avatarCid || undefined} />
              <div className="wx-join-preview-meta">
                <h3 className="wx-join-preview-title">{lookup.name}</h3>
                <p className="wx-join-preview-sub">
                  #{lookup.groupId}
                  {lookup.exists && (
                    <>
                      {" · "}
                      {lookup.isPublic ? "公开群" : "私群"}
                      {" · "}
                      {lookup.memberCount} 人
                      {lookup.frozen ? " · 已冻结" : ""}
                    </>
                  )}
                </p>
                {lookup.exists && (
                  <span className={`wx-join-status-badge status-${lookup.status}`}>
                    {joinStatusLabel(lookup.status)}
                  </span>
                )}
              </div>
            </div>

            {!lookup.exists ? (
              <p className="dm-hint wx-group-warn">群 #{lookup.groupId} 不存在，请检查 ID。</p>
            ) : (
              <>
                {lookup.announcement && (
                  <div className="wx-join-announcement">
                    <span className="wx-join-announcement-label">公告</span>
                    <p>{lookup.announcement}</p>
                  </div>
                )}
                {lookup.adminAddress && (
                  <p className="wx-group-pick-hint">
                    群主：{shortNexAddress(lookup.adminAddress)}
                  </p>
                )}
                {lookup.isPublic && (
                  <p className="dm-hint wx-group-warn">
                    这是公开群，无法自助申请。请联系群主或管理员将你加入。
                  </p>
                )}
                {lookup.status === "pending_request" && (
                  <p className="dm-hint">申请已提交，管理员批准后会将你加入群。</p>
                )}
              </>
            )}

            <div className="wx-join-cta-stack">{renderCta()}</div>

            <button
              type="button"
              className="wx-join-back-btn"
              disabled={busy}
              onClick={() => {
                setStep("input");
                setLookup(null);
                setLocalError(null);
              }}
            >
              ← 重新输入 ID
            </button>
          </>
        ) : null}

        {(localError || (!chainOk && step === "input")) && (
          <p className="wallet-error wx-group-error">{localError}</p>
        )}
      </aside>
    </div>
  );
}
