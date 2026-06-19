import { useCallback, useEffect, useState } from "react";
import { chainClient } from "@/chain/chainClient";
import { config } from "@/config";
import type { GroupJoinRequestRow } from "@/group/groupJoinTypes";
import { useContactRoster } from "@/hooks/useContactRoster";
import { useAppStore } from "@/state/appStore";
import { removeRequiresSwap } from "@/mls/groupMemberFlow";
import { canonicalAddress, shortNexAddress } from "@/wallet/address";
import { unlockDesktopSession } from "@/wallet/session";
import { TxPasswordSheet } from "@/ui/wallet/TxPasswordSheet";
import type { GroupRole } from "@/types/viewModels";

export interface GroupManageTarget {
  groupId: number;
  title: string;
  memberCount: number;
  myRole: GroupRole;
  initialTab?: "members" | "joinRequests";
}

function roleLabel(role: GroupRole): string {
  switch (role) {
    case "owner":
      return "群主";
    case "admin":
      return "管理员";
    case "member":
      return "成员";
    default:
      return "";
  }
}

type ManageTab = "members" | "joinRequests";

type ChainSignAction = "disband" | "leave";

const CHAIN_SIGN_TITLES: Record<ChainSignAction, string> = {
  disband: "签名解散群聊",
  leave: "签名退出群聊",
};

// EN: Remove / swap / leave + approve private join requests. CN: 成员管理 + 私群入群审批。
export function GroupManageModal() {
  const {
    groupManageTarget,
    closeGroupManage,
    removeGroupMember,
    swapGroupMember,
    leaveGroupChat,
    disbandGroupChat,
    approveJoinRequests,
    account,
  } = useAppStore();
  const roster = useContactRoster();
  const [tab, setTab] = useState<ManageTab>("members");
  const [members, setMembers] = useState<
    { address: string; role: "owner" | "admin" | "member" }[]
  >([]);
  const [joinRequests, setJoinRequests] = useState<GroupJoinRequestRow[]>([]);
  const [selectedApplicants, setSelectedApplicants] = useState<Set<string>>(new Set());
  const [loadingMembers, setLoadingMembers] = useState(false);
  const [loadingRequests, setLoadingRequests] = useState(false);
  const [removeTarget, setRemoveTarget] = useState<string | null>(null);
  const [replacePick, setReplacePick] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [mode, setMode] = useState<"list" | "swap" | "disbandConfirm">("list");
  const [signAction, setSignAction] = useState<ChainSignAction | null>(null);

  const open = groupManageTarget != null;
  const target = groupManageTarget;
  const chainOk =
    !config.useMock && config.mlsControlPlane === "chain" && config.mlsBackend === "openmls";
  const canManageOthers =
    target != null && (target.myRole === "owner" || target.myRole === "admin");
  const canLeave = target != null && target.myRole !== "owner";
  const canDisband = target != null && target.myRole === "owner";
  const mustSwap =
    target != null && removeTarget != null
      ? removeRequiresSwap(target.memberCount, 1)
      : false;
  const soloGroupNeedsMore =
    target != null && target.memberCount === 1 && selectedApplicants.size > 0 && selectedApplicants.size < 2;

  const loadJoinRequests = useCallback(async (groupId: number) => {
    setLoadingRequests(true);
    try {
      const rows = await chainClient.listGroupJoinRequests(groupId);
      setJoinRequests(rows);
      setSelectedApplicants(new Set());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoadingRequests(false);
    }
  }, []);

  useEffect(() => {
    if (!open || !target || config.useMock) {
      setMembers([]);
      setJoinRequests([]);
      return;
    }
    let cancelled = false;
    setLoadingMembers(true);
    void chainClient.listGroupMembers(target.groupId).then((rows) => {
      if (!cancelled) setMembers(rows);
      if (!cancelled) setLoadingMembers(false);
    });
    void loadJoinRequests(target.groupId);
    return () => {
      cancelled = true;
    };
  }, [open, target?.groupId, loadJoinRequests]);

  useEffect(() => {
    if (!open || !target) return;
    setTab(target.initialTab ?? "members");
    setRemoveTarget(null);
    setReplacePick(null);
    setMode("list");
    setError(null);
    setProgress(null);
    setSelectedApplicants(new Set());
    setSignAction(null);
  }, [open, target?.groupId, target?.initialTab]);

  if (!open || !target) return null;

  const labelFor = (address: string) => {
    if (account && canonicalAddress(address) === canonicalAddress(account.account)) return "我";
    const hit = roster.find((m) => canonicalAddress(m.address) === canonicalAddress(address));
    return hit?.labels[0] ?? shortNexAddress(address);
  };

  const peers = roster.filter(
    (m) =>
      m.address !== account?.account &&
      !members.some((row) => canonicalAddress(row.address) === canonicalAddress(m.address)),
  );

  const toggleApplicant = (address: string) => {
    const canon = canonicalAddress(address);
    setSelectedApplicants((prev) => {
      const next = new Set(prev);
      if (next.has(canon)) next.delete(canon);
      else next.add(canon);
      return next;
    });
  };

  const onApproveSelected = async () => {
    if (!chainOk || busy || selectedApplicants.size === 0) return;
    setBusy(true);
    setError(null);
    try {
      await approveJoinRequests(target.groupId, [...selectedApplicants], {
        onProgress: (message) => setProgress(message),
      });
      await loadJoinRequests(target.groupId);
      setTab("members");
      closeGroupManage();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const onApproveOne = async (address: string) => {
    if (!chainOk || busy) return;
    setBusy(true);
    setError(null);
    try {
      await approveJoinRequests(target.groupId, [address], {
        onProgress: (message) => setProgress(message),
      });
      await loadJoinRequests(target.groupId);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const onRemoveOnly = async () => {
    if (!removeTarget || !chainOk || busy || mustSwap) return;
    setBusy(true);
    setError(null);
    try {
      await removeGroupMember(removeTarget, {
        onProgress: (message) => setProgress(message),
      });
      closeGroupManage();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const onSwap = async () => {
    if (!removeTarget || !replacePick || !chainOk || busy) return;
    setBusy(true);
    setError(null);
    try {
      await swapGroupMember(removeTarget, replacePick, {
        onProgress: (message) => setProgress(message),
      });
      closeGroupManage();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const runAfterWalletPassword = async (password: string, action: ChainSignAction) => {
    if (!account) throw new Error("请先解锁账户");
    await unlockDesktopSession(account.account, password);
    setBusy(true);
    setError(null);
    try {
      if (action === "disband") {
        await disbandGroupChat({ onProgress: (message) => setProgress(message) });
      } else {
        await leaveGroupChat({ onProgress: (message) => setProgress(message) });
      }
      closeGroupManage();
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const onLeave = () => {
    if (!chainOk || busy || !canLeave) return;
    setError(null);
    setSignAction("leave");
  };

  const onDisband = () => {
    if (!chainOk || busy || !canDisband) return;
    setError(null);
    setSignAction("disband");
  };

  return (
    <div className="dm-overlay tg-modal-overlay" onClick={() => !busy && closeGroupManage()}>
      <aside className="dm-panel tg-modal wx-group-modal" onClick={(e) => e.stopPropagation()}>
        <header className="dm-head">
          <span>成员管理</span>
          <button type="button" onClick={() => !busy && closeGroupManage()} disabled={busy}>
            ✕
          </button>
        </header>

        {!chainOk ? (
          <p className="dm-hint wx-group-warn">成员管理需要链上 MLS 控制面。</p>
        ) : (
          <>
            <p className="dm-hint">
              「{target.title}」· {target.memberCount} 人 · 我的角色：{roleLabel(target.myRole)}
            </p>

            {canManageOthers && (
              <div className="wx-group-manage-tabs">
                <button
                  type="button"
                  className={`wx-group-manage-tab${tab === "members" ? " on" : ""}`}
                  disabled={busy}
                  onClick={() => setTab("members")}
                >
                  成员
                </button>
                <button
                  type="button"
                  className={`wx-group-manage-tab${tab === "joinRequests" ? " on" : ""}`}
                  disabled={busy}
                  onClick={() => setTab("joinRequests")}
                >
                  入群申请
                  {joinRequests.length > 0 && (
                    <span className="wx-group-manage-tab-badge">{joinRequests.length}</span>
                  )}
                </button>
              </div>
            )}

            {tab === "joinRequests" && canManageOthers ? (
              <>
                {target.memberCount === 1 && (
                  <p className="dm-hint wx-group-warn">
                    当前群仅有群主，须一次批准并加入至少 2 人（链上禁止 2 人群）。
                  </p>
                )}
                {loadingRequests ? (
                  <p className="dm-hint">正在加载入群申请…</p>
                ) : joinRequests.length === 0 ? (
                  <p className="dm-empty">暂无入群申请</p>
                ) : (
                  <>
                    <ul className="dm-list wx-group-pick-list wx-join-req-list">
                      {joinRequests.map((row) => {
                        const canon = canonicalAddress(row.address);
                        const on = selectedApplicants.has(canon);
                        const canSingleApprove =
                          target.memberCount !== 1 || joinRequests.length === 1;
                        return (
                          <li key={row.address} className="wx-join-req-row">
                            <button
                              type="button"
                              className={`wx-group-pick-row wx-join-req-pick${on ? " on" : ""}`}
                              disabled={busy}
                              onClick={() => toggleApplicant(row.address)}
                            >
                              <span className={`wx-group-check${on ? " on" : ""}`}>
                                {on ? "✓" : ""}
                              </span>
                              <span className="wx-join-req-main">
                                <span className="dm-name">{labelFor(row.address)}</span>
                                <span className="wx-join-req-tags">
                                  {row.approved ? (
                                    <span className="wx-join-req-tag approved">已批准</span>
                                  ) : (
                                    <span className="wx-join-req-tag pending">待批准</span>
                                  )}
                                  {!row.hasKeyPackage && (
                                    <span className="wx-join-req-tag warn">无 KeyPackage</span>
                                  )}
                                </span>
                              </span>
                            </button>
                            {canSingleApprove && target.memberCount !== 1 && (
                              <button
                                type="button"
                                className="wx-group-manage-action"
                                disabled={busy || !row.hasKeyPackage}
                                onClick={() => void onApproveOne(row.address)}
                              >
                                批准并加入
                              </button>
                            )}
                          </li>
                        );
                      })}
                    </ul>
                    <button
                      type="button"
                      className="tg-add-contact-btn wx-group-create-btn"
                      disabled={
                        busy ||
                        selectedApplicants.size === 0 ||
                        soloGroupNeedsMore ||
                        [...selectedApplicants].some((addr) => {
                          const row = joinRequests.find(
                            (r) => canonicalAddress(r.address) === addr,
                          );
                          return row && !row.hasKeyPackage;
                        })
                      }
                      onClick={() => void onApproveSelected()}
                    >
                      {busy
                        ? "处理中…"
                        : `批准并加入所选 (${selectedApplicants.size}${soloGroupNeedsMore ? "，至少 2 人" : ""})`}
                    </button>
                    {soloGroupNeedsMore && (
                      <p className="wx-group-pick-hint">请再选择至少 1 位申请人</p>
                    )}
                    {selectedApplicants.size > 0 &&
                      [...selectedApplicants].some((addr) => {
                        const row = joinRequests.find(
                          (r) => canonicalAddress(r.address) === addr,
                        );
                        return row && !row.hasKeyPackage;
                      }) && (
                        <p className="wx-group-pick-hint wx-group-warn">
                          所选申请人须已发布 KeyPackage（请让对方解锁 NexChat）
                        </p>
                      )}
                  </>
                )}
              </>
            ) : (
              <>
                {loadingMembers ? (
                  <p className="dm-hint">正在加载成员…</p>
                ) : (
                  <ul className="dm-list wx-group-pick-list">
                    {members.map((m) => (
                      <li key={m.address} className="wx-group-member-manage-row">
                        <span className="dm-name">{labelFor(m.address)}</span>
                        <span className="dm-addr">{roleLabel(m.role)}</span>
                        {canManageOthers &&
                          m.role !== "owner" &&
                          account &&
                          canonicalAddress(m.address) !== canonicalAddress(account.account) && (
                            <button
                              type="button"
                              className="wx-group-manage-action"
                              disabled={busy}
                              onClick={() => {
                                setRemoveTarget(m.address);
                                setMode(
                                  mustSwap || removeRequiresSwap(target.memberCount, 1)
                                    ? "swap"
                                    : "list",
                                );
                                if (removeRequiresSwap(target.memberCount, 1)) setMode("swap");
                              }}
                            >
                              {removeRequiresSwap(target.memberCount, 1) ? "替换" : "移除"}
                            </button>
                          )}
                      </li>
                    ))}
                  </ul>
                )}

                {canManageOthers && removeTarget && (
                  <div className="wx-group-manage-panel">
                    <p className="wx-group-pick-hint">
                      目标：{labelFor(removeTarget)}
                      {mustSwap || removeRequiresSwap(target.memberCount, 1)
                        ? "（3 人群须同时添加 1 位替换成员）"
                        : ""}
                    </p>
                    {(mustSwap || mode === "swap") && (
                      <>
                        <p className="wx-group-pick-hint">选择替换成员</p>
                        <ul className="dm-list wx-group-pick-list">
                          {peers.map((m) => {
                            const canon = canonicalAddress(m.address);
                            const on = replacePick === canon;
                            return (
                              <li key={m.address}>
                                <button
                                  type="button"
                                  className={`wx-group-pick-row${on ? " on" : ""}`}
                                  onClick={() => setReplacePick(canon)}
                                  disabled={busy}
                                >
                                  <span className={`wx-group-check${on ? " on" : ""}`}>
                                    {on ? "✓" : ""}
                                  </span>
                                  <span className="dm-name">{m.labels[0] ?? m.ref}</span>
                                </button>
                              </li>
                            );
                          })}
                        </ul>
                        <button
                          type="button"
                          className="tg-add-contact-btn wx-group-create-btn"
                          disabled={!replacePick || busy}
                          onClick={() => void onSwap()}
                        >
                          {busy ? "处理中…" : "移除并替换"}
                        </button>
                      </>
                    )}
                    {!mustSwap && mode === "list" && !removeRequiresSwap(target.memberCount, 1) && (
                      <button
                        type="button"
                        className="tg-add-contact-btn wx-group-create-btn danger"
                        disabled={busy}
                        onClick={() => void onRemoveOnly()}
                      >
                        {busy ? "处理中…" : "确认移除"}
                      </button>
                    )}
                  </div>
                )}
              </>
            )}

            {canLeave && tab === "members" && mode !== "disbandConfirm" && (
              <button
                type="button"
                className="wx-group-leave-btn"
                disabled={busy || removeRequiresSwap(target.memberCount, 1)}
                onClick={onLeave}
              >
                {busy && signAction === "leave" ? "处理中…" : "退出群聊"}
              </button>
            )}

            {canDisband && tab === "members" && (
              <>
                {mode !== "disbandConfirm" ? (
                  <button
                    type="button"
                    className="tg-add-contact-btn wx-group-create-btn danger wx-group-disband-btn"
                    disabled={busy}
                    onClick={() => {
                      setError(null);
                      setMode("disbandConfirm");
                    }}
                  >
                    解散群聊
                  </button>
                ) : (
                  <div className="wx-group-manage-panel wx-group-disband-panel">
                    <p className="wx-group-pick-hint wx-group-warn">
                      解散后群聊将被永久删除，建群押金将退还至你的账户。此操作不可撤销。
                    </p>
                    <p className="wx-group-pick-hint">
                      确认解散「{target.title}」？（{target.memberCount} 人）
                    </p>
                    <button
                      type="button"
                      className="tg-add-contact-btn wx-group-create-btn danger"
                      disabled={busy}
                      onClick={onDisband}
                    >
                      {busy && signAction === "disband" ? "处理中…" : "确认解散群聊"}
                    </button>
                    <button
                      type="button"
                      className="wx-group-leave-btn"
                      disabled={busy}
                      onClick={() => setMode("list")}
                    >
                      取消
                    </button>
                  </div>
                )}
              </>
            )}

            {error && <p className="wallet-error wx-group-error">{error}</p>}
            {progress && busy && !signAction && <p className="wx-group-progress">{progress}</p>}
          </>
        )}
      </aside>

      <TxPasswordSheet
        open={signAction != null}
        title={signAction ? CHAIN_SIGN_TITLES[signAction] : "链上签名"}
        hint="输入钱包密码以签名并提交链上交易（解散/退群需链上确认）。"
        onClose={() => {
          if (busy) return;
          setSignAction(null);
        }}
        onConfirm={async (password) => {
          if (!signAction) return;
          try {
            await runAfterWalletPassword(password, signAction);
            setSignAction(null);
          } catch (e) {
            setError(e instanceof Error ? e.message : String(e));
            throw e;
          }
        }}
      />
    </div>
  );
}
