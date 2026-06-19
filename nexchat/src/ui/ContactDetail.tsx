import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { useContactRoster } from "@/hooks/useContactRoster";
import { peerAvatarCid, usePeerAvatarMap } from "@/hooks/usePeerAvatarMap";
import { isUserContact } from "@/store/contactBook";
import { Avatar } from "@/ui/Avatar";
import { nexDisplayAddress, shortNexAddress } from "@/wallet/address";

import { directMlsDetailHint } from "@/mls/directMlsUi";
export function ContactDetail({ address }: { address: string }) {
  const roster = useContactRoster();
  const { account, directMls, drPeers, startDirectChat, removeContact } = useAppStore();
  const setMainTab = useUiStore((s) => s.setMainTab);
  const selectContact = useUiStore((s) => s.selectContact);

  const member = roster.find((m) => m.address === address);
  const avatarMap = usePeerAvatarMap([address], !!member);
  const avatarCid = peerAvatarCid(avatarMap, address);

  if (!member) {
    return (
      <main className="tg-main tg-main-empty">
        <div className="tg-empty-state">
          <p>联系人不存在</p>
          <button type="button" className="tg-welcome-primary" onClick={() => selectContact(null)}>
            返回列表
          </button>
        </div>
      </main>
    );
  }

  const title = member.labels[0] ?? member.ref;
  const mls = directMls[address];
  const dr = drPeers[address] ?? false;
  const ready = dr || (mls?.ready ?? false);
  const canRemove = account ? isUserContact(account.account, address) : false;

  return (
    <main className="tg-main tg-contact-detail">
      <header className="tg-sub-head">
        <button type="button" className="tg-sub-back" onClick={() => selectContact(null)}>
          ← 联系人
        </button>
        <span>{title}</span>
      </header>

      <div className="tg-contact-hero">
        <Avatar kind="direct" title={title} avatarCid={avatarCid} className="tg-contact-avatar" />
        <h2>{title}</h2>
        <p className="tg-contact-handle">@{member.ref}</p>
        <p className="tg-contact-addr">{nexDisplayAddress(address)}</p>
        <p className="tg-contact-addr-sm">{shortNexAddress(address, 8, 6)}</p>

        <div className={`tg-contact-mls${ready ? " ok" : ""}`}>
          {dr ? "🔒 私聊 E2EE 就绪 (Double Ratchet)" : directMlsDetailHint(mls)}
        </div>

        <button
          type="button"
          className="tg-contact-msg-btn"
          onClick={() => {
            void startDirectChat(address, title);
            setMainTab("chats");
          }}
        >
          💬 发消息
        </button>

        {canRemove && (
          <button
            type="button"
            className="tg-contact-remove-btn"
            onClick={() => {
              if (!confirm(`从通讯录移除 ${title}？`)) return;
              void removeContact(address);
              selectContact(null);
            }}
          >
            从通讯录移除
          </button>
        )}
      </div>

      <section className="tg-profile-section">
        <h3>信息</h3>
        <div className="tg-profile-row">
          <span className="tg-profile-row-label">MLS 角色</span>
          <span className="tg-profile-row-value">{mls?.role ?? "—"}</span>
        </div>
        <div className="tg-profile-row">
          <span className="tg-profile-row-label">别名</span>
          <span className="tg-profile-row-value mono">
            {member.ref}, {nexDisplayAddress(address)}
          </span>
        </div>
      </section>
    </main>
  );
}
