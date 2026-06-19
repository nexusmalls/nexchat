// EN: Add a contact by SS58 address + display name (saved per account in localStorage).
// CN: 用 SS58 地址 + 显示名添加联系人（按账户存入 localStorage）。

import { useState } from "react";
import { useAppStore } from "@/state/appStore";

export function AddContactDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const addContact = useAppStore((s) => s.addContact);
  const [label, setLabel] = useState("");
  const [address, setAddress] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) return null;

  function reset() {
    setLabel("");
    setAddress("");
    setError(null);
    setBusy(false);
  }

  function close() {
    reset();
    onClose();
  }

  return (
    <div className="dm-overlay tg-modal-overlay" onClick={close}>
      <aside className="dm-panel tg-modal add-contact-panel" onClick={(e) => e.stopPropagation()}>
        <header className="dm-head">
          <span>添加联系人</span>
          <button type="button" onClick={close}>
            ✕
          </button>
        </header>
        <p className="dm-hint">
          输入对方的链上 SS58 地址（NEX 钱包以 X 开头，dev 账户以 5 开头，均可识别）。
          对方在线且 relay 可达时会收到添加通知。
        </p>
        {error && <p className="wallet-error">{error}</p>}
        <div className="add-contact-form">
          <label className="wallet-field">
            <span>显示名称</span>
            <input
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="例如 Carol"
              autoFocus
            />
          </label>
          <label className="wallet-field">
            <span>链上地址 (SS58)</span>
            <input
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="5F… 或 nex…"
              className="mono-input"
            />
          </label>
          <button
            type="button"
            className="wallet-primary"
            disabled={busy || label.trim().length < 1 || address.trim().length < 10}
            onClick={() =>
              void (async () => {
                setBusy(true);
                setError(null);
                try {
                  await addContact(address, label.trim());
                  close();
                } catch (e) {
                  setError(e instanceof Error ? e.message : String(e));
                } finally {
                  setBusy(false);
                }
              })()
            }
          >
            {busy ? "保存中…" : "添加到通讯录"}
          </button>
        </div>
      </aside>
    </div>
  );
}
