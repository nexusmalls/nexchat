// EN: Track A PIN-wrapped signing-key backup enrollment UI (design §5.3 path C, P1). Primary devices
// seal `exportSigningKeys()` under a user-chosen PIN and publish the pointer to relay (+ chain manifest).
// CN: 路线 A PIN 包裹签名钥备份注册 UI（设计 §5.3 路径 C，P1）。主设备用用户 PIN 密封
// `exportSigningKeys()` 并发布指针到 relay（+ 链上 manifest）。

import { useEffect, useState } from "react";
import { useTranslations } from "@/i18n";
import { config, signingPinBackupActive } from "@/config";
import { openMlsEngine } from "@/mls/openMlsEngine";
import {
  SIGNING_PIN_MAX_LEN,
  SIGNING_PIN_MIN_LEN,
  normalizeSigningPin,
} from "@/mls/signingPinBackup";
import { readLocalMlsSigningPointer } from "@/store/mlsSigningBackupSync";
import { useAppStore } from "@/state/appStore";

function formatBackupTime(ts: number): string {
  try {
    return new Date(ts).toLocaleString();
  } catch {
    return String(ts);
  }
}

/// EN: Settings section — create or rotate the offline signing-key backup. CN: 设置区块——创建或轮换离线签名钥备份。
export function SigningPinBackupPanel() {
  const t = useTranslations("settings");
  const account = useAppStore((s) => s.account?.account ?? "");
  const groupSendMode = useAppStore((s) => s.groupSendMode);
  const createSigningPinBackup = useAppStore((s) => s.createSigningPinBackup);

  const enabled = signingPinBackupActive();

  const [pin, setPin] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [lastBackupAt, setLastBackupAt] = useState<number | null>(null);

  const canExport = openMlsEngine.canExportEscrow();
  const readOnly = config.mlsVaultEnabled && (!canExport || groupSendMode === "secondary");

  useEffect(() => {
    if (!account) {
      setLastBackupAt(null);
      return;
    }
    const local = readLocalMlsSigningPointer(account);
    setLastBackupAt(local?.updated_at ?? null);
  }, [account, success]);

  if (!enabled) return null;

  return (
    <section className="tg-signing-pin-backup">
      <p className="tg-settings-row-label">{t("signingPinTitle")}</p>
      <p className="tg-settings-note">{t("signingPinDesc")}</p>

      <div className="tg-profile-row">
        <span className="tg-profile-row-label">{t("signingPinLastBackup")}</span>
        <span className="tg-profile-row-value">
          {lastBackupAt ? formatBackupTime(lastBackupAt) : t("signingPinNever")}
        </span>
      </div>

      {readOnly ? (
        <p className="tg-settings-note">{t("signingPinReadOnly")}</p>
      ) : (
        <>
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
                setSuccess(null);
              }}
              placeholder={`${SIGNING_PIN_MIN_LEN}–${SIGNING_PIN_MAX_LEN}`}
            />
          </label>
          <label className="wx-market-field">
            <span>{t("signingPinConfirm")}</span>
            <input
              className="wx-wallet-input"
              type="password"
              inputMode="numeric"
              autoComplete="off"
              maxLength={SIGNING_PIN_MAX_LEN}
              value={confirm}
              onChange={(e) => {
                setConfirm(e.target.value.replace(/\D/g, ""));
                setError(null);
                setSuccess(null);
              }}
            />
          </label>
          {error && <p className="wx-market-tx-status error">{error}</p>}
          {success && <p className="wx-market-tx-status ok">{success}</p>}
          <button
            type="button"
            className="tg-handoff-btn"
            disabled={busy || !account}
            onClick={() =>
              void (async () => {
                if (!account) return;
                try {
                  normalizeSigningPin(pin);
                } catch (e) {
                  setError(e instanceof Error ? e.message : String(e));
                  return;
                }
                if (pin !== confirm) {
                  setError(t("signingPinMismatch"));
                  return;
                }
                setBusy(true);
                setError(null);
                setSuccess(null);
                try {
                  const ptr = await createSigningPinBackup(pin);
                  setLastBackupAt(ptr.updated_at);
                  setPin("");
                  setConfirm("");
                  setSuccess(t("signingPinSuccess"));
                } catch (e) {
                  setError(e instanceof Error ? e.message : String(e));
                } finally {
                  setBusy(false);
                }
              })()
            }
          >
            {busy ? t("signingPinSubmitting") : t("signingPinSubmit")}
          </button>
        </>
      )}
    </section>
  );
}
