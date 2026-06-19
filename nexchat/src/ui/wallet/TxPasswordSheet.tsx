import { useEffect, useState } from "react";

// EN: Password gate before submitting a chain extrinsic with the built-in desktop wallet.
// CN: 内置桌面钱包提交链上 extrinsic 前的密码确认弹窗。
export function TxPasswordSheet({
  open,
  title,
  hint,
  onClose,
  onConfirm,
}: {
  open: boolean;
  title: string;
  hint?: string;
  onClose: () => void;
  onConfirm: (password: string) => Promise<void>;
}) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setPassword("");
      setBusy(false);
      setError(null);
    }
  }, [open]);

  if (!open) return null;

  return (
    <div className="wx-wallet-modal-backdrop" onClick={() => !busy && onClose()}>
      <div
        className="wx-wallet-modal"
        role="dialog"
        aria-modal
        aria-labelledby="tx-password-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="wx-wallet-modal-head">
          <h3 id="tx-password-title">{title}</h3>
          <button type="button" className="wx-wallet-modal-close" disabled={busy} onClick={onClose}>
            ✕
          </button>
        </header>
        <div className="wx-wallet-modal-body">
          <p className="wx-wallet-modal-hint">
            {hint ?? "输入钱包密码以签名并提交链上交易。"}
          </p>
          <label className="wx-market-field">
            <span>钱包密码</span>
            <input
              className="wx-wallet-input"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="current-password"
              autoFocus
              disabled={busy}
              onKeyDown={(e) => {
                if (e.key === "Enter" && password && !busy) {
                  void submit();
                }
              }}
            />
          </label>
          {error && <p className="wx-market-tx-status error">{error}</p>}
          <button
            type="button"
            className="wx-market-submit buy"
            disabled={busy || !password}
            onClick={() => void submit()}
          >
            {busy ? "签名提交中…" : "签名并提交"}
          </button>
          <button type="button" className="wx-wallet-link-btn" disabled={busy} onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  );

  async function submit() {
    if (!password || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onConfirm(password);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
}
