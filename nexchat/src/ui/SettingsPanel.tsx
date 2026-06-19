import { useTranslations } from "@/i18n";
import { useAppStore } from "@/state/appStore";
import { useUiStore, type SettingsView } from "@/state/uiStore";
import { useWallet } from "@/hooks/useWallet";
import { config } from "@/config";
import { SidebarHeader } from "@/ui/SidebarHeader";
import { shortAddress } from "@/wallet/address";

const ROWS: { view: SettingsView; icon: string; labelKey: string; hintKey: string }[] = [
  { view: "profile", icon: "👤", labelKey: "profile", hintKey: "profileHint" },
  { view: "privacy", icon: "🔒", labelKey: "privacy", hintKey: "privacyHint" },
  { view: "notifications", icon: "🔔", labelKey: "notifications", hintKey: "notificationsHint" },
  { view: "data", icon: "💾", labelKey: "data", hintKey: "dataHint" },
  { view: "about", icon: "ℹ️", labelKey: "about", hintKey: "aboutHint" },
];

// EN: Settings tab sidebar — navigates detail panes in the main column.
// CN: 设置 Tab 侧栏——在主栏打开详情页。
export function SettingsPanel() {
  const t = useTranslations("settings");
  const tApp = useTranslations("app");
  const settingsView = useUiStore((s) => s.settingsView);
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const { account } = useAppStore();
  const { name, address, source, lock } = useWallet();
  const setPinsOpen = useAppStore((s) => s.setPinsOpen);

  const displayName = name ?? account?.nickname ?? tApp("defaultUser");

  return (
    <aside className="tg-sidebar">
      <SidebarHeader title={t("title")} />

      <button
        type="button"
        className="tg-settings-profile-card"
        onClick={() => setSettingsView("profile")}
      >
        <div className="tg-settings-profile-avatar">{displayName[0]?.toUpperCase() ?? "?"}</div>
        <div className="tg-settings-profile-meta">
          <span className="tg-settings-profile-name">{displayName}</span>
          <span className="tg-settings-profile-sub">
            {address ? shortAddress(address) : account?.account ? shortAddress(account.account) : "—"}
          </span>
        </div>
        <span className="tg-settings-chevron">›</span>
      </button>

      <div className="tg-settings-list">
        <button
          type="button"
          className={`tg-settings-row${settingsView === "language" ? " active" : ""}`}
          onClick={() => setSettingsView("language")}
        >
          <span className="tg-settings-row-icon">🌐</span>
          <span className="tg-settings-row-body">
            <span className="tg-settings-row-label">{t("language")}</span>
            <span className="tg-settings-row-hint">{t("languageDesc")}</span>
          </span>
          <span className="tg-settings-chevron">›</span>
        </button>
        {ROWS.map((r) => (
          <button
            key={r.view}
            type="button"
            className={`tg-settings-row${settingsView === r.view ? " active" : ""}`}
            onClick={() => setSettingsView(r.view)}
          >
            <span className="tg-settings-row-icon">{r.icon}</span>
            <span className="tg-settings-row-body">
              <span className="tg-settings-row-label">{t(r.labelKey)}</span>
              {r.hintKey && <span className="tg-settings-row-hint">{t(r.hintKey)}</span>}
            </span>
            <span className="tg-settings-chevron">›</span>
          </button>
        ))}
      </div>

      <div className="tg-settings-list tg-settings-actions">
        {!config.useMock && (
          <button type="button" className="tg-settings-row" onClick={() => setPinsOpen(true)}>
            <span className="tg-settings-row-icon">📌</span>
            <span className="tg-settings-row-body">
              <span className="tg-settings-row-label">{t("ipfsPin")}</span>
            </span>
            <span className="tg-settings-chevron">›</span>
          </button>
        )}
        <button type="button" className="tg-settings-row danger" onClick={lock}>
          <span className="tg-settings-row-icon">🔒</span>
          <span className="tg-settings-row-body">
            <span className="tg-settings-row-label">{t("lockWallet")}</span>
            <span className="tg-settings-row-hint">
              {source === "dev" ? t("devAccount") : t("desktopKeyring")}
            </span>
          </span>
        </button>
      </div>
    </aside>
  );
}
