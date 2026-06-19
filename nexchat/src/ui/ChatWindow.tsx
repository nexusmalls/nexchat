import { useEffect, useMemo, useRef, useState } from "react";
import { formatChatDisplayName } from "@/chat/displayName";
import { useContactRoster } from "@/hooks/useContactRoster";
import { useMobileComposer } from "@/hooks/useMobileComposer";
import { useAppStore } from "@/state/appStore";
import { WeChatNavBar } from "@/ui/WeChatNavBar";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { openMlsEngine } from "@/mls/openMlsEngine";
import {
  ensureGroupMlsErrorMessage,
  ensureGroupMlsReady,
} from "@/mls/joinGroupMlsFlow";
import type { MessageVM } from "@/types/viewModels";
import { MediaContent } from "@/ui/MediaContent";
import { MentionText } from "@/ui/MentionText";
import { ProductShareBubble } from "@/ui/ProductShareBubble";
import { parseProductShare } from "@/shop/shareCard";
import { VoiceComposer } from "@/ui/VoiceComposer";
import { ChatAttachSheet } from "@/ui/ChatAttachSheet";
import { WireDeviceSheet } from "@/ui/WireDeviceSheet";
import { WireGroupDeviceSheet } from "@/ui/WireGroupDeviceSheet";
import { prepareGroupWireConversation, removeGroupWireDevice, wireDeviceRosterFor, wireGroupRosterFor } from "@/state/appStore";
import { AdminJoinRequestBanner } from "@/ui/AdminJoinRequestBanner";
import { SigningPinRestoreButton } from "@/ui/SigningPinRestoreButton";
import { canForwardMessage } from "@/p3/forward";
import { canRecallMessage } from "@/p3/recall";
import { contentPreviewFromMessage, isDegradedReplyQuote, replyQuotePreview } from "@/ui/messagePreview";
import { voiceRecordingSupported } from "@/voice/voiceRecorder";

/// EN: Cross-device delete propagation needs msg-archive + IPFS; adjust confirm copy when off.
/// CN: 跨设备删除同步依赖 msg-archive + IPFS；关闭时调整确认文案。
function clearConversationConfirmText(): string {
  const peerNote = "对端设备上的副本不会因此被删除。";
  if (config.msgArchiveEnabled && config.ipfsEnabled) {
    return `清空本会话的聊天记录？此操作仅删除本地消息，并同步删除你其他设备上的记录。${peerNote}`;
  }
  return `清空本会话的聊天记录？此操作仅删除本设备上的本地消息。${peerNote}`;
}

function deleteMessageConfirmText(): string {
  const peerNote = "对端设备上的副本不会因此被删除。";
  if (config.msgArchiveEnabled && config.ipfsEnabled) {
    return `删除这条消息？此操作仅删除本地消息，并同步删除你其他设备上的该消息。${peerNote}`;
  }
  return `删除这条消息？此操作仅删除本设备上的该消息。${peerNote}`;
}

function deleteConversationConfirmText(isGroup: boolean): string {
  const peerNote = "对端设备上的副本不会因此被删除。";
  const groupNote = isGroup
    ? "你仍在该群内，收到新消息时会话会重新出现。"
    : "重新发起私聊时会话会重新出现。";
  if (config.msgArchiveEnabled && config.ipfsEnabled && config.convIndexEnabled) {
    return `删除此会话？将从聊天列表移除并清空本地记录，并同步到你其他设备。${groupNote}${peerNote}`;
  }
  if (config.msgArchiveEnabled && config.ipfsEnabled) {
    return `删除此会话？将从聊天列表移除并清空本地记录，并同步清空你其他设备上的消息。${groupNote}${peerNote}`;
  }
  return `删除此会话？将从聊天列表移除并清空本设备上的记录。${groupNote}${peerNote}`;
}

// EN: Telegram-style chat pane — header, bubble timeline, pill composer.
// CN: Telegram 风格聊天区——顶栏、气泡时间线、圆角输入框。
export function ChatWindow() {
  const {
    activeConvId,
    conversations,
    messages,
    sendMessage,
    sendFile,
    closeConversation,
    account,
    setGroupAvatar,
    openGroupManage,
    openInviteGroupMembers,
    groupJoinRequestCounts,
    replyingTo,
    setReplyingTo,
    openForwardPicker,
    deleteMessage,
    recallMessage,
    clearConversationHistory,
    deleteConversation,
    ephemeralMs,
    setEphemeral,
    mls,
    directMls,
    drPeers,
    mlsSyncRev,
    groupSendMode,
    requestGroupSendAuthority,
  } = useAppStore();
  const roster = useContactRoster();
  const mobileComposer = useMobileComposer();
  const [draft, setDraft] = useState("");
  const [inputMode, setInputMode] = useState<"text" | "voice">("text");
  const [voiceError, setVoiceError] = useState<string | null>(null);
  const [attachSheetOpen, setAttachSheetOpen] = useState(false);
  const [deviceSheetOpen, setDeviceSheetOpen] = useState(false);
  const [groupMlsHint, setGroupMlsHint] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const avatarInputRef = useRef<HTMLInputElement>(null);
  const timelineRef = useRef<HTMLDivElement>(null);

  const conv = conversations.find((c) => c.convId === activeConvId);

  const displayNameOpts = useMemo(
    () => ({
      selfNickname: account?.nickname,
      selfAddress: account?.account,
      fallbackAddress: conv?.kind === "direct" ? conv.peer : undefined,
    }),
    [account?.nickname, account?.account, conv?.kind, conv?.peer],
  );

  const labelSender = (senderRef: string) =>
    formatChatDisplayName(senderRef, roster, displayNameOpts);

  useEffect(() => {
    const el = timelineRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages, activeConvId]);

  const isGroupOpenMls =
    !!conv && config.mlsBackend === "openmls" && conv.kind === "group";
  const isDirectOpenMls =
    !!conv && config.mlsBackend === "openmls" && conv.kind === "direct" && !!conv.peer;
  // EN: A DR-pinned 1:1 is E2EE-ready immediately (no blocking handshake, §21) → treat as ready.
  // CN: DR 钉定的 1:1 立即 E2EE 就绪（无阻塞握手，§21）→ 视为就绪。
  const isDrConv = conv?.peer ? (drPeers[conv.peer] ?? false) : false;
  const directReady = conv?.peer
    ? isDrConv || (directMls[conv.peer]?.ready ?? false)
    : true;
  const groupReady =
    isGroupOpenMls && activeConvId
      ? (() => {
          try {
            return openMlsEngine.hasGroup(activeConvId);
          } catch {
            return mls?.ready ?? false;
          }
        })()
      : false;
  const mlsReady =
    !conv || (!isGroupOpenMls && !isDirectOpenMls)
      ? true
      : isDirectOpenMls
        ? directReady
        : groupReady;

  useEffect(() => {
    if (!isGroupOpenMls || !activeConvId || !account?.account || groupReady) {
      setGroupMlsHint(null);
      return;
    }
    const gid = Number(activeConvId.slice(2));
    if (!Number.isFinite(gid)) return;

    let alive = true;
    const tick = async () => {
      if (config.wireGroupMultileafEnabled) {
        await prepareGroupWireConversation(activeConvId);
        try {
          if (openMlsEngine.hasGroup(activeConvId)) {
            setGroupMlsHint(null);
            useAppStore.setState((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
            return;
          }
        } catch {
          /* fall through */
        }
        setGroupMlsHint("正在接入群加密设备，请稍候…");
        return;
      }
      const result = await ensureGroupMlsReady({
        engine: openMlsEngine,
        chain: chainClient,
        selfAddress: account.account,
        groupId: gid,
      });
      if (!alive) return;
      if (result.ok) {
        setGroupMlsHint(null);
        useAppStore.setState((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
        return;
      }
      setGroupMlsHint(ensureGroupMlsErrorMessage(result));
    };

    void tick();
    const id = setInterval(() => void tick(), 3000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [activeConvId, account?.account, isGroupOpenMls, groupReady, mlsSyncRev]);

  const visibleMessages = useMemo(
    () => messages.filter((m) => m.content.type !== "reaction"),
    [messages],
  );

  // EN: 1:1 Wire multi-leaf device roster for the security-disclosure UX (design §8). Recomputed on
  // `mlsSyncRev` so adding/removing a device refreshes the count + list. Null off-feature / non-1:1 /
  // no group. CN: 1:1 Wire 多 leaf 设备名册，供安全披露 UX（设计 §8）。随 `mlsSyncRev` 重算，使增/删设备
  // 刷新计数与列表。非功能/非 1:1/无群时为 null。
  const wireRoster = useMemo(
    () => (isDirectOpenMls && activeConvId ? wireDeviceRosterFor(activeConvId) : null),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isDirectOpenMls, activeConvId, mlsSyncRev],
  );

  // EN: Group Wire multi-leaf device roster for the security-disclosure UX (design §9). Null off-feature
  // (`wireGroupMultileafEnabled`) / non-group / no group held. Recomputed on `mlsSyncRev`. CN: 群 Wire
  // 多 leaf 设备名册，供安全披露 UX（设计 §9）。非功能（`wireGroupMultileafEnabled`）/非群/未持群时为 null。
  // 随 `mlsSyncRev` 重算。
  const wireGroupRoster = useMemo(
    () => (isGroupOpenMls && activeConvId ? wireGroupRosterFor(activeConvId) : null),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isGroupOpenMls, activeConvId, mlsSyncRev],
  );

  if (!conv) {
    // EN: activeConvId set but merge list not ready yet (slow RPC / cold start) — still show
    // back so mobile users are not trapped without the conversation list.
    // CN: 已选会话但列表尚未合并完成（RPC 慢/冷启动）——仍显示返回，避免移动端无法退回列表。
    if (activeConvId) {
      return (
        <main className="tg-main wx-chat-main">
          <WeChatNavBar title="加载中…" onBack={() => closeConversation()} />
          <div className="tg-empty-state">
            <p>正在加载会话…</p>
          </div>
        </main>
      );
    }
    return (
      <main className="tg-main tg-main-empty">
        <div className="tg-empty-state">
          <div className="tg-empty-icon">💬</div>
          <h2>选择一个会话</h2>
          <p>端到端加密聊天 · 链上控制面 + OpenMLS</p>
          <p className="tg-empty-hint">开多个标签页用 //Alice、//Bob 体验链上群握手</p>
        </div>
      </main>
    );
  }

  // EN: Track A send-authority — a group on a read-only (escrow-restored) device can be READ but not
  // sent to until an online handoff installs the signing key (§5.4/§7.3). `groupSendMode` is `primary`
  // unless the escrow vault is on, so non-escrow chats are unaffected. CN: 路线 A 发送权——只读（托管恢复）
  // 设备上的群可**读**但不可发送，直到在线交接装入签名钥（§5.4/§7.3）。未启用托管 vault 时 `groupSendMode`
  // 恒为 `primary`，非托管会话不受影响。
  // EN: Track A read-only send block is RETIRED on the group side under group Wire (design §10): every
  // device holds its own MLS leaf and can always send, so there is no "read-only secondary" state.
  // CN: 群 Wire 下轨 A 只读发送阻断在群侧**退役**（设计 §10）：每台设备各持自己的 MLS leaf 恒可发送，无
  // 「只读副设备」状态。
  const groupSendBlocked =
    isGroupOpenMls && !config.wireGroupMultileafEnabled && groupSendMode !== "primary";
  const canSend = !conv.adminMuted && !conv.frozen && mlsReady && !groupSendBlocked;
  const blockReason = conv.frozen
    ? "群已冻结，只读"
    : conv.adminMuted
      ? "你已被管理员禁言"
      : !mlsReady
        ? groupMlsHint ??
          (isDirectOpenMls ? "正在建立 1:1 OpenMLS 会话…" : "正在同步群加密状态…")
        : groupSendBlocked
          ? groupSendMode === "restoring"
            ? "正在恢复本设备的群发送权…"
            : "此设备为只读（已从云端恢复群聊）"
          : "";

  const onSend = () => {
    const text = draft.trim();
    if (!text || !canSend) return;
    void sendMessage(text);
    setDraft("");
  };

  const previewOf = (id?: string) =>
    id ? messages.find((m) => m.clientMsgId === id) : undefined;

  const showVoiceMode =
    mobileComposer && config.ipfsEnabled && voiceRecordingSupported();

  const chatTitle =
    conv.kind === "group"
      ? conv.title
      : labelSender(conv.title || conv.peer || "用户");

  // EN: device-count disclosure: a Wire multi-leaf conv (1:1 or group) can show how many devices are in
  // the conversation. CN: 设备数披露：Wire 多 leaf 会话（1:1 或群）可显示会话内设备台数。
  const wireDeviceCount = wireRoster?.total ?? wireGroupRoster?.total ?? 0;
  const encLabel = isDrConv
    ? "端到端加密 · 私聊 (DR)"
    : wireDeviceCount > 1
      ? `端到端加密 · ${wireDeviceCount} 台设备`
      : "端到端加密";
  const statusLine =
    conv.frozen
      ? "群已冻结"
      : conv.dnd
        ? "免打扰"
        : isDirectOpenMls || isGroupOpenMls
          ? mlsReady
            ? encLabel
            : "正在连接…"
          : conv.kind === "group"
            ? `${conv.memberCount} 名成员`
            : "在线";

  const headActions = (
    <>
      {conv.kind === "group" &&
        conv.groupId != null &&
        !conv.frozen &&
        (conv.myRole === "owner" || conv.myRole === "admin") && (
          <button
            className="wx-nav-icon-btn"
            title="邀请成员"
            type="button"
            onClick={() =>
              openInviteGroupMembers({
                groupId: conv.groupId!,
                title: conv.title,
                memberCount: conv.memberCount,
              })
            }
          >
            ➕
          </button>
        )}
      {conv.kind === "group" && conv.groupId != null && !conv.frozen && (
        <button
          className="wx-nav-icon-btn wx-nav-badge-host"
          title="成员管理"
          type="button"
          onClick={() =>
            openGroupManage({
              groupId: conv.groupId!,
              title: conv.title,
              memberCount: conv.memberCount,
              myRole: conv.myRole,
              initialTab:
                (groupJoinRequestCounts[conv.groupId!] ?? 0) > 0 &&
                (conv.myRole === "owner" || conv.myRole === "admin")
                  ? "joinRequests"
                  : "members",
            })
          }
        >
          👥
          {(groupJoinRequestCounts[conv.groupId] ?? 0) > 0 &&
            (conv.myRole === "owner" || conv.myRole === "admin") && (
              <span className="wx-nav-badge">{groupJoinRequestCounts[conv.groupId]}</span>
            )}
        </button>
      )}
      {conv.kind === "group" &&
        conv.groupId != null &&
        (conv.myRole === "owner" || conv.myRole === "admin") &&
        config.ipfsEnabled && (
          <>
            <button
              className="wx-nav-icon-btn"
              title="群头像"
              type="button"
              onClick={() => avatarInputRef.current?.click()}
            >
              🖼
            </button>
            <input
              ref={avatarInputRef}
              type="file"
              accept="image/*"
              hidden
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f && conv.groupId != null) void setGroupAvatar(conv.groupId, f);
                e.target.value = "";
              }}
            />
          </>
        )}
      {((isDirectOpenMls && wireRoster) || (isGroupOpenMls && wireGroupRoster)) && (
        <button
          className="wx-nav-icon-btn wx-nav-badge-host"
          title="设备与安全"
          type="button"
          onClick={() => setDeviceSheetOpen(true)}
        >
          🛡
          {wireDeviceCount > 1 && <span className="wx-nav-badge">{wireDeviceCount}</span>}
        </button>
      )}
      {activeConvId && (
        <>
          <button
            className="wx-nav-icon-btn"
            title="清空聊天记录"
            type="button"
            onClick={() => {
              if (confirm(clearConversationConfirmText())) {
                void clearConversationHistory(activeConvId);
              }
            }}
          >
            🗑
          </button>
          <button
            className="wx-nav-icon-btn"
            title="删除会话"
            type="button"
            onClick={() => {
              if (confirm(deleteConversationConfirmText(conv?.kind === "group"))) {
                void deleteConversation(activeConvId);
              }
            }}
          >
            ✕
          </button>
        </>
      )}
      {(isGroupOpenMls || isDirectOpenMls) && (
        <span className={`tg-mls-badge${mlsReady ? " ok" : ""}`}>
          {mlsReady ? "🔒" : "⏳"}
        </span>
      )}
    </>
  );

  return (
    <main className="tg-main wx-chat-main">
      <WeChatNavBar
        title={chatTitle}
        onBack={() => closeConversation()}
        actions={headActions}
      />
      <div className="wx-chat-subline">{statusLine}</div>
      {isDirectOpenMls && wireRoster && (
        <WireDeviceSheet
          open={deviceSheetOpen}
          convId={activeConvId!}
          roster={wireRoster}
          peerTitle={chatTitle}
          onClose={() => setDeviceSheetOpen(false)}
        />
      )}
      {isGroupOpenMls && wireGroupRoster && activeConvId && (
        <WireGroupDeviceSheet
          open={deviceSheetOpen}
          convId={activeConvId}
          roster={wireGroupRoster}
          groupTitle={chatTitle}
          onRemove={removeGroupWireDevice}
          onClose={() => setDeviceSheetOpen(false)}
        />
      )}
      {conv.kind === "group" && conv.groupId != null && (
        <AdminJoinRequestBanner groupId={conv.groupId} />
      )}

      <div className="tg-timeline" ref={timelineRef}>
        {visibleMessages.map((m) => (
          <Bubble
            key={m.clientMsgId}
            msg={m}
            senderLabel={labelSender(m.senderRef)}
            showSenderName={conv.kind === "group"}
            replyQuote={replyQuotePreview(m.replyTo, previewOf(m.replyTo))}
            convTitle={
              m.forwardFrom
                ? conversations.find((c) => c.convId === m.forwardFrom?.convId)?.title ??
                  m.forwardFrom.convId
                : undefined
            }
            onReply={() => setReplyingTo(m)}
            onForward={() => openForwardPicker(m)}
            onRecall={() => {
              if (confirm("撤回这条消息？撤回后对方将看到「消息已撤回」。")) {
                void recallMessage(m);
              }
            }}
            onDelete={() => {
              if (confirm(deleteMessageConfirmText())) {
                void deleteMessage(m);
              }
            }}
            onKeep={() => void useAppStore.getState().keepAttachment(m)}
            onBodyDownloaded={
              !m.isOutgoing
                ? () => void useAppStore.getState().ackMediaDownloaded(m.convId, m.clientMsgId)
                : undefined
            }
          />
        ))}
        {visibleMessages.length === 0 && (
          <div className="tg-timeline-empty">还没有消息，发送第一条吧</div>
        )}
      </div>

      <footer className="tg-composer-wrap">
        <ChatAttachSheet
          open={attachSheetOpen}
          onClose={() => setAttachSheetOpen(false)}
          onFile={(f) => void sendFile(f)}
        />
        {replyingTo && (
          <div className="tg-reply-bar">
            <span className="tg-reply-label">
              回复 {labelSender(replyingTo.senderRef)}：{contentPreviewFromMessage(replyingTo)}
            </span>
            <button className="tg-reply-x" onClick={() => setReplyingTo(null)} type="button">
              ✕
            </button>
          </div>
        )}
        {voiceError && <p className="wx-voice-composer-error">{voiceError}</p>}
        {canSend ? (
          <div className="tg-composer">
            {config.ipfsEnabled && (
              <>
                <button
                  className="tg-composer-icon"
                  title="附件"
                  type="button"
                  onClick={() => {
                    if (mobileComposer) setAttachSheetOpen(true);
                    else fileInputRef.current?.click();
                  }}
                >
                  📎
                </button>
                {!mobileComposer && (
                  <input
                    ref={fileInputRef}
                    type="file"
                    hidden
                    onChange={(e) => {
                      const f = e.target.files?.[0];
                      if (f) void sendFile(f);
                      e.target.value = "";
                    }}
                  />
                )}
              </>
            )}
            {showVoiceMode && (
              <button
                className="tg-composer-icon wx-composer-mode-btn"
                title={inputMode === "voice" ? "切换到键盘" : "切换到语音"}
                type="button"
                onClick={() => {
                  setInputMode((m) => (m === "voice" ? "text" : "voice"));
                  setVoiceError(null);
                }}
              >
                {inputMode === "voice" ? "⌨" : "🎤"}
              </button>
            )}
            {inputMode === "voice" && showVoiceMode ? (
              <VoiceComposer
                disabled={!canSend}
                onSend={sendFile}
                onError={(message) => setVoiceError(message)}
              />
            ) : (
              <>
                <button
                  className={`tg-composer-icon${ephemeralMs ? " on" : ""}`}
                  title="阅后即焚"
                  type="button"
                  onClick={() => setEphemeral(ephemeralMs ? null : 60000)}
                >
                  ⏱
                </button>
                <input
                  className="tg-composer-input"
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && onSend()}
                  placeholder={ephemeralMs ? "阅后即焚消息…" : "输入消息"}
                />
                <button
                  className="tg-send-btn"
                  type="button"
                  onClick={onSend}
                  disabled={!draft.trim()}
                  aria-label="发送"
                >
                  ➤
                </button>
              </>
            )}
          </div>
        ) : groupSendBlocked && groupSendMode === "secondary" ? (
          <div className="tg-composer-blocked">
            <span>{blockReason}</span>
            <SigningPinRestoreButton />
            <button
              type="button"
              className="tg-handoff-btn"
              onClick={() => void requestGroupSendAuthority()}
              title="向你的主设备申请发送权（在线交接签名密钥）"
            >
              在此设备发送
            </button>
          </div>
        ) : (
          <div className="tg-composer-blocked">{blockReason}</div>
        )}
      </footer>
    </main>
  );
}

function Bubble({
  msg,
  senderLabel,
  showSenderName,
  replyQuote,
  convTitle,
  onReply,
  onForward,
  onRecall,
  onDelete,
  onKeep,
  onBodyDownloaded,
}: {
  msg: MessageVM;
  senderLabel: string;
  showSenderName: boolean;
  replyQuote?: string;
  convTitle?: string;
  onReply: () => void;
  onForward: () => void;
  onRecall?: () => void;
  onDelete?: () => void;
  onKeep?: () => void;
  onBodyDownloaded?: () => void;
}) {
  if (msg.content.type === "system") {
    return <div className="tg-bubble tg-bubble-system">🔔 {msg.content.kind}</div>;
  }
  // EN: recalled messages render a neutral placeholder for both sides; only delete (local
  // cleanup) stays available. CN: 已撤回消息对双方渲染中性占位；仅保留删除（本地清理）。
  if (msg.status === "recalled") {
    return (
      <div className={`tg-bubble${msg.isOutgoing ? " out" : " in"} tg-bubble-recalled`}>
        {showSenderName && !msg.isOutgoing && (
          <div className="tg-bubble-sender">{senderLabel}</div>
        )}
        <div className="tg-bubble-text tg-recalled-text">🚫 消息已撤回</div>
        <div className="tg-bubble-foot">
          {onDelete && (
            <button className="tg-bubble-action" onClick={onDelete} type="button" title="删除">
              🗑
            </button>
          )}
        </div>
      </div>
    );
  }
  return (
    <div className={`tg-bubble${msg.isOutgoing ? " out" : " in"}`}>
      {showSenderName && !msg.isOutgoing && (
        <div className="tg-bubble-sender">{senderLabel}</div>
      )}
      {replyQuote && (
        <div
          className={`tg-quote${isDegradedReplyQuote(replyQuote) ? " tg-quote-muted" : ""}`}
        >
          ↪ {replyQuote}
        </div>
      )}
      {msg.forwardFrom && (
        <div className="tg-forward-card">
          <span className="tg-forward-label">转发 · {convTitle ?? msg.forwardFrom.convId}</span>
          <span className="tg-forward-preview">
            {msg.forwardFrom.preview ?? msg.forwardFrom.msgId}
          </span>
        </div>
      )}
      <div className="tg-bubble-text">
        {msg.content.type === "media" ? (
          <MediaContent
            content={msg.content}
            uploading={msg.isOutgoing && msg.status === "pending" && !msg.content.bodyReady}
            onBodyDownloaded={onBodyDownloaded}
          />
        ) : msg.content.type === "text" && parseProductShare(msg.content.text) ? (
          <ProductShareBubble text={msg.content.text} />
        ) : msg.content.type === "text" ? (
          <MentionText text={msg.content.text} />
        ) : (
          contentPreviewFromMessage(msg)
        )}
      </div>
      <div className="tg-bubble-foot">
        {msg.mentions.length > 0 && (
          <span className="mention-tag">@{msg.mentions.join(" @")}</span>
        )}
        {msg.ephemeralBurnAt && <EphemeralCountdown burnAt={msg.ephemeralBurnAt} />}
        <span className="tg-bubble-time">{statusLabel(msg)}</span>
        <button className="tg-bubble-action" onClick={onReply} type="button" title="回复">
          ↩
        </button>
        {canForwardMessage(msg) && (
          <button className="tg-bubble-action" onClick={onForward} type="button" title="转发">
            ➡
          </button>
        )}
        {onRecall && canRecallMessage(msg) && (
          <button className="tg-bubble-action" onClick={onRecall} type="button" title="撤回">
            ↺
          </button>
        )}
        {msg.content.type === "media" && !msg.ephemeralTtlMs && onKeep && (
          <button
            className="tg-bubble-action"
            onClick={onKeep}
            type="button"
            title={msg.starred ? "已保留" : "保留附件（豁免清理 + 链上 Pin）"}
            disabled={msg.starred}
          >
            {msg.starred ? "★" : "☆"}
          </button>
        )}
        {onDelete && (
          <button className="tg-bubble-action" onClick={onDelete} type="button" title="删除消息">
            🗑
          </button>
        )}
      </div>
    </div>
  );
}

function EphemeralCountdown({ burnAt }: { burnAt: number }) {
  const [left, setLeft] = useState(() => Math.max(0, burnAt - Date.now()));
  useEffect(() => {
    const t = setInterval(() => setLeft(Math.max(0, burnAt - Date.now())), 500);
    return () => clearInterval(t);
  }, [burnAt]);
  const sec = Math.ceil(left / 1000);
  return <span className="eph-tag">🔥 {sec > 0 ? `${sec}s` : "…"}</span>;
}

function statusLabel(m: MessageVM): string {
  if (m.source === "onChainSystem") return "链上";
  switch (m.status) {
    case "pending":
      return "…";
    case "sent":
      return "✓";
    case "acked":
      return "✓✓";
    case "failed":
      return "✗";
    case "recalled":
      return "撤回";
  }
}
