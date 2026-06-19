// EN: Track A group send-key management — a persistent Me/Settings entry (design §5.2/§5.3/§7.3).
// A FULL device (holds the signing key) can back up that key under a PIN here; a READ-ONLY device
// (escrow-restored, no signing key) regains send authority here via offline PIN restore or the §5
// online handoff. Surfacing both in one always-present screen means a stranded device always has a
// recovery path, not just a transient banner.
// CN: 路线 A 群聊发送密钥管理——常驻的「我的/设置」入口（设计 §5.2/§5.3/§7.3）。完整设备（持签名钥）可在此
// 用 PIN 备份该密钥；只读设备（托管恢复、无签名钥）可在此经离线 PIN 恢复或 §5 在线交接重获发送权。两者集中
// 在一个常驻页面，使被卡住的设备始终有恢复入口，而非只靠转瞬即逝的横幅。

import { useTranslations } from "@/i18n";
import { config, signingPinBackupActive } from "@/config";
import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { SigningPinBackupPanel } from "@/ui/SigningPinBackupPanel";
import { SigningPinRestoreButton } from "@/ui/SigningPinRestoreButton";

export function SendingKeyPanel() {
  const t = useTranslations("settings");
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const groupSendMode = useAppStore((s) => s.groupSendMode);
  const requestGroupSendAuthority = useAppStore((s) => s.requestGroupSendAuthority);
  const setNotice = useAppStore((s) => s.setNotice);

  // EN: read-only (escrow-restored) device that has not yet regained sending authority. CN: 尚未重获
  // 发送权的只读（托管恢复）设备。
  const readOnly = config.mlsVaultEnabled && groupSendMode === "secondary";

  return (
    <main className="tg-main tg-settings-main">
      <header className="tg-sub-head">
        <button type="button" className="tg-sub-back" onClick={() => setSettingsView("list")}>
          {t("back")}
        </button>
        <span>{t("signingPinTitle")}</span>
      </header>
      <div className="tg-settings-detail">
        {/* EN: full device → PIN backup form; read-only device → the read-only note (self-gated). CN:
            完整设备→PIN 备份表单；只读设备→只读说明（自带门控）。 */}
        <SigningPinBackupPanel />

        {readOnly && (
          <section className="tg-signing-pin-backup">
            <p className="tg-settings-row-label">恢复本设备发送权</p>
            <p className="tg-settings-note">
              {signingPinBackupActive()
                ? "此设备为只读（已从云端恢复），暂时无法在群聊发送。可用你在主设备上设置的 PIN 离线恢复，或请仍持有签名钥的在线主设备进行在线交接。"
                : "此设备为只读（已从云端恢复），暂时无法在群聊发送。请仍持有签名钥的在线主设备进行在线交接。"}
            </p>
            <SigningPinRestoreButton
              onRestored={() => {
                // EN: signing key installed → this device can send again; confirm + return to list.
                // CN: 签名钥已装入→本设备恢复发送；提示并返回列表。
                setNotice("已恢复本设备发送权");
                setSettingsView("list");
              }}
            />
            <button
              type="button"
              className="tg-handoff-btn"
              onClick={() => void requestGroupSendAuthority()}
              title="向你的主设备申请发送权（在线交接签名密钥）"
            >
              在此设备发送（在线交接）
            </button>
          </section>
        )}
      </div>
    </main>
  );
}
