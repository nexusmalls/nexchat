// EN: Track A PIN restore sheet (design §5.3 path C, P2). Decrypts the offline signing backup and
// installs keys on a read-only device. CN: 路线 A PIN 恢复面板（设计 §5.3 路径 C，P2）。在只读设备上解密
// 离线签名备份并装入密钥。

import { useState } from "react";
import { useTranslations } from "@/i18n";
import {
  SIGNING_PIN_MAX_LEN,
  SIGNING_PIN_MIN_LEN,
  normalizeSigningPin,
} from "@/mls/signingPinBackup";
import { useAppStore } from "@/state/appStore";

export function SigningPinRestoreSheet({
  open,
  onClose,
  onRestored,
}: {
  open: boolean;
  onClose: () => void;
  onRestored?: () => void;
}) {
  const t = useTranslations("settings");
  const restoreSigningPinBackup = useAppStore((s) => s.restoreSigningPinBackup);
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) return null;

  return (
    <div className="dm-overlay tg-modal-overlay" onClick={() => !busy && onClose()}>
      <aside className="dm-panel tg-modal wx-pin-restore-modal" onClick={(e) => e.stopPropagation()}>
        <header className="dm-head">
          <span>{t("signingPinRestoreTitle")}</span>
          <button type="button" onClick={() => !busy && onClose()} disabled={busy}>
            ✕
          </button>
        </header>
        <p className="dm-hint">{t("signingPinRestoreDesc")}</p>
        <label className="wx-market-field">
          <span>{t("signingPinLabel")}</span>
          <input
            className="wx-wallet-input"
            type="password"
            inputMode="numeric"
            autoComplete="off"
            maxLength={SIGNING_PIN_MAX_LEN}
            value={pin}
            onChange={(e) => {
              setPin(e.target.value.replace(/\D/g, ""));
              setError(null);
            }}
            placeholder={`${SIGNING_PIN_MIN_LEN}–${SIGNING_PIN_MAX_LEN}`}
            disabled={busy}
          />
        </label>
        {error && <p className="wx-market-tx-status error">{error}</p>}
        <button
          type="button"
          className="tg-handoff-btn wx-pin-restore-submit"
          disabled={busy || !pin}
          onClick={() =>
            void (async () => {
              try {
                normalizeSigningPin(pin);
              } catch (e) {
                setError(e instanceof Error ? e.message : String(e));
                return;
              }
              setBusy(true);
              setError(null);
              try {
                await restoreSigningPinBackup(pin);
                setPin("");
                onRestored?.();
                onClose();
              } catch (e) {
                setError(e instanceof Error ? e.message : String(e));
              } finally {
                setBusy(false);
              }
            })()
          }
        >
          {busy ? t("signingPinRestoreSubmitting") : t("signingPinRestoreSubmit")}
        </button>
      </aside>
    </div>
  );
}
