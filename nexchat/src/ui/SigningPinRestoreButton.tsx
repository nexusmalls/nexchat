// EN: Entry button for offline PIN signing-key restore (P2). Shown on read-only devices when a backup
// pointer exists. CN: 离线 PIN 签名钥恢复入口按钮（P2）。只读设备且存在备份指针时显示。

import { useEffect, useState } from "react";
import { useTranslations } from "@/i18n";
import { signingPinBackupActive } from "@/config";
import { openMlsEngine } from "@/mls/openMlsEngine";
import { hasSigningPinBackup } from "@/store/mlsSigningBackupSync";
import { useAppStore } from "@/state/appStore";
import { SigningPinRestoreSheet } from "@/ui/SigningPinRestoreSheet";

export function SigningPinRestoreButton({
  disabled,
  className = "tg-handoff-btn",
  onRestored,
}: {
  disabled?: boolean;
  className?: string;
  onRestored?: () => void;
}) {
  const t = useTranslations("settings");
  const account = useAppStore((s) => s.account?.account ?? "");
  const groupSendMode = useAppStore((s) => s.groupSendMode);
  const [offered, setOffered] = useState(false);
  const [open, setOpen] = useState(false);

  const featureOn = signingPinBackupActive();
  const readOnly =
    featureOn &&
    (!openMlsEngine.canExportEscrow() || groupSendMode === "secondary");

  useEffect(() => {
    if (!readOnly || !account) {
      setOffered(false);
      return;
    }
    let alive = true;
    void hasSigningPinBackup(account).then((ok) => {
      if (alive) setOffered(ok);
    });
    return () => {
      alive = false;
    };
  }, [readOnly, account, open]);

  if (!readOnly || !offered) return null;

  return (
    <>
      <button
        type="button"
        className={className}
        disabled={disabled}
        onClick={() => setOpen(true)}
        title={t("signingPinRestoreDesc")}
      >
        {t("signingPinRestoreBtn")}
      </button>
      <SigningPinRestoreSheet
        open={open}
        onClose={() => setOpen(false)}
        onRestored={onRestored}
      />
    </>
  );
}
