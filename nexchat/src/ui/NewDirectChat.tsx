// EN: Start a 1:1 direct chat with any contact (roster + user address book).
// CN: 与通讯录中任意联系人发起 1:1 私聊。

import { useState } from "react";
import { useAppStore } from "@/state/appStore";
import { useContactRoster } from "@/hooks/useContactRoster";
import { AddContactDialog } from "@/ui/AddContactDialog";

export function NewDirectChat() {
  const { newDmOpen, setNewDmOpen, account, startDirectChat, directMls, drPeers } = useAppStore();
  const roster = useContactRoster();
  const [addOpen, setAddOpen] = useState(false);

  if (!newDmOpen) return null;

  const peers = roster.filter((m) => m.address !== account?.account);

  return (
    <>
      <div className="dm-overlay tg-modal-overlay" onClick={() => setNewDmOpen(false)}>
        <aside className="dm-panel tg-modal" onClick={(e) => e.stopPropagation()}>
          <header className="dm-head">
            <span>新聊天</span>
            <button type="button" onClick={() => setNewDmOpen(false)}>
              ✕
            </button>
          </header>
          <p className="dm-hint">1:1 私聊走链下 OpenMLS，不建链上群。需双方在线并完成握手。</p>
          <button
            type="button"
            className="tg-add-contact-btn dm-add-contact"
            onClick={() => setAddOpen(true)}
          >
            ＋ 添加联系人
          </button>
          <ul className="dm-list">
            {peers.map((m) => {
              const dr = drPeers[m.address] ?? false;
              const ready = dr || directMls[m.address]?.ready;
              return (
                <li key={m.address}>
                  <button
                    type="button"
                    className="dm-peer"
                    onClick={() => void startDirectChat(m.address, m.labels[0] ?? m.ref)}
                  >
                    <span className="dm-name">{m.labels[0] ?? m.ref}</span>
                    <span className="dm-addr">{m.address.slice(0, 10)}…</span>
                    <span className={`dm-mls ${ready ? "ready" : ""}`}>
                      {dr ? "私聊 ✓" : ready ? "MLS ✓" : "握手中…"}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
          {peers.length === 0 && (
            <p className="dm-empty">通讯录为空，请先添加联系人</p>
          )}
        </aside>
      </div>
      <AddContactDialog
        open={addOpen}
        onClose={() => setAddOpen(false)}
      />
    </>
  );
}
