import { useMemo, useState } from "react";
import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { useContactRoster } from "@/hooks/useContactRoster";
import { peerAvatarCid, usePeerAvatarMap } from "@/hooks/usePeerAvatarMap";
import type { MentionMember } from "@/p3/mentions";
import type { ConversationVM, GroupRole } from "@/types/viewModels";
import { Avatar } from "@/ui/Avatar";
import { WeChatNavBar } from "@/ui/WeChatNavBar";
import { AddContactDialog } from "@/ui/AddContactDialog";
import { ContactRequestsSection } from "@/ui/ContactRequestsSection";
import { GroupInvitesSection } from "@/ui/GroupInvitesSection";
import { nexDisplayAddress, shortNexAddress } from "@/wallet/address";

import { directMlsBadgeText } from "@/mls/directMlsUi";
import type { DirectMlsStatus } from "@/mls/directHandshake";

/// EN: Group row MLS badge — chain/group Wire encryption readiness (not 1:1 peer online).
/// CN: 群聊行 MLS 标记——链上/群 Wire 加密是否就绪（非 1:1 对端在线）。
function groupMlsBadge(ready: boolean): string {
  return ready ? "E2EE 就绪" : "群加密未就绪";
}

// EN: Contacts tab — peers + joined groups, search, add by SS58.
// CN: 联系人 Tab——私聊联系人 + 已加入群聊，搜索，按 SS58 添加。
export function ContactsPanel() {
  const roster = useContactRoster();
  const conversations = useAppStore((s) => s.conversations);
  const directMls = useAppStore((s) => s.directMls);
  const drPeers = useAppStore((s) => s.drPeers);
  const mls = useAppStore((s) => s.mls);
  const selectedContact = useUiStore((s) => s.selectedContact);
  const selectedGroupConvId = useUiStore((s) => s.selectedGroupConvId);
  const selectContact = useUiStore((s) => s.selectContact);
  const selectGroup = useUiStore((s) => s.selectGroup);
  const [query, setQuery] = useState("");
  const [addOpen, setAddOpen] = useState(false);

  const q = query.trim().toLowerCase();

  const peers = useMemo(() => {
    if (!q) return roster;
    return roster.filter(
      (m) =>
        m.labels.some((l) => l.toLowerCase().includes(q)) ||
        m.ref.toLowerCase().includes(q) ||
        m.address.toLowerCase().includes(q) ||
        nexDisplayAddress(m.address).toLowerCase().includes(q),
    );
  }, [roster, q]);

  const groups = useMemo(() => {
    const joined = conversations
      .filter((c) => c.kind === "group")
      .sort((a, b) => a.title.localeCompare(b.title, "zh-CN"));
    if (!q) return joined;
    return joined.filter(
      (c) =>
        c.title.toLowerCase().includes(q) ||
        String(c.groupId ?? "").includes(q) ||
        `${c.memberCount}`.includes(q),
    );
  }, [conversations, q]);

  const grouped = useMemo(() => groupByLetter(peers), [peers]);
  const peerAddresses = useMemo(() => roster.map((m) => m.address), [roster]);
  const avatarMap = usePeerAvatarMap(peerAddresses);
  const subtitle =
    groups.length > 0
      ? `${peers.length} 位联系人 · ${groups.length} 个群`
      : `${peers.length} 位联系人`;

  return (
    <aside className="tg-sidebar wx-panel">
      <WeChatNavBar
        title="通讯录"
        actions={
          <button type="button" className="wx-nav-icon-btn wx-nav-plus" onClick={() => setAddOpen(true)} title="添加联系人">
            ⊕
          </button>
        }
      />
      <p className="wx-panel-subtitle">{subtitle}</p>

      <div className="tg-search-wrap">
        <span className="tg-search-icon" aria-hidden>
          🔍
        </span>
        <input
          className="tg-search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索联系人或群聊"
        />
      </div>

      <AddContactDialog open={addOpen} onClose={() => setAddOpen(false)} />

      <div className="tg-chat-list tg-contacts-list">
        <ContactRequestsSection />
        <GroupInvitesSection />
        {groups.length > 0 && (
          <section className="tg-contact-section tg-contact-groups">
            <div className="tg-contact-letter">群聊</div>
            {groups.map((g) => (
              <GroupRow
                key={g.convId}
                conv={g}
                active={selectedGroupConvId === g.convId}
                mlsReady={mls?.ready ?? false}
                onClick={() => selectGroup(g.convId)}
              />
            ))}
          </section>
        )}
        {Object.entries(grouped).map(([letter, members]) => (
          <section key={letter} className="tg-contact-section">
            <div className="tg-contact-letter">{letter}</div>
            {members.map((m) => (
              <ContactRow
                key={m.address}
                member={m}
                avatarCid={peerAvatarCid(avatarMap, m.address)}
                active={selectedContact === m.address}
                mlsStatus={directMls[m.address]}
                dr={drPeers[m.address] ?? false}
                onClick={() => selectContact(m.address)}
              />
            ))}
          </section>
        ))}
        {peers.length === 0 && groups.length === 0 && (
          <div className="tg-list-empty">
            {query ? "无匹配联系人或群聊" : "暂无联系人，点上方添加"}
          </div>
        )}
      </div>
    </aside>
  );
}

function groupRoleLabel(role: GroupRole): string {
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

function GroupRow({
  conv,
  active,
  mlsReady,
  onClick,
}: {
  conv: ConversationVM;
  active: boolean;
  mlsReady: boolean;
  onClick: () => void;
}) {
  const role = groupRoleLabel(conv.myRole);
  return (
    <button
      type="button"
      className={`tg-chat-row tg-contact-row tg-group-contact-row${active ? " active" : ""}`}
      onClick={onClick}
    >
      <Avatar kind="group" title={conv.title} avatarCid={conv.avatarCid} />
      <div className="tg-chat-row-body">
        <div className="tg-chat-row-top">
          <span className="tg-chat-name">{conv.title}</span>
          <span className={`tg-contact-status${mlsReady ? " online" : ""}`}>
            {groupMlsBadge(mlsReady)}
          </span>
        </div>
        <div className="tg-chat-row-bottom">
          <span className="tg-chat-preview">
            {conv.memberCount} 名成员{role ? ` · ${role}` : ""}
            {conv.unread > 0 ? ` · ${conv.unread} 未读` : ""}
          </span>
        </div>
      </div>
    </button>
  );
}

function ContactRow({
  member,
  avatarCid,
  active,
  mlsStatus,
  dr,
  onClick,
}: {
  member: MentionMember;
  avatarCid?: string;
  active: boolean;
  mlsStatus?: DirectMlsStatus;
  dr?: boolean;
  onClick: () => void;
}) {
  const title = member.labels[0] ?? member.ref;
  // EN: DR-pinned 1:1 is E2EE-ready immediately (§21) — show a distinct private-chat badge. CN: DR
  // 钉定的 1:1 立即 E2EE 就绪（§21）——显示独立的私聊标记。
  const ready = dr || (mlsStatus?.ready ?? false);
  return (
    <button
      type="button"
      className={`tg-chat-row tg-contact-row${active ? " active" : ""}`}
      onClick={onClick}
    >
      <Avatar kind="direct" title={title} avatarCid={avatarCid} />
      <div className="tg-chat-row-body">
        <div className="tg-chat-row-top">
          <span className="tg-chat-name">{title}</span>
          <span className={`tg-contact-status${ready ? " online" : ""}`}>
            {dr ? "私聊 E2EE 就绪" : directMlsBadgeText(mlsStatus)}
          </span>
        </div>
        <div className="tg-chat-row-bottom">
          <span className="tg-chat-preview">@{member.ref} · {shortNexAddress(member.address)}</span>
        </div>
      </div>
    </button>
  );
}

function groupByLetter(members: MentionMember[]): Record<string, MentionMember[]> {
  const out: Record<string, MentionMember[]> = {};
  for (const m of members) {
    const label = m.labels[0] ?? m.ref;
    const letter = /^[A-Za-z\u4e00-\u9fff]/.test(label)
      ? /^[A-Za-z]/.test(label)
        ? label[0]!.toUpperCase()
        : "#"
      : "#";
    (out[letter] ??= []).push(m);
  }
  for (const k of Object.keys(out)) {
    out[k]!.sort((a, b) =>
      (a.labels[0] ?? a.ref).localeCompare(b.labels[0] ?? b.ref),
    );
  }
  return Object.fromEntries(Object.entries(out).sort(([a], [b]) => a.localeCompare(b)));
}
