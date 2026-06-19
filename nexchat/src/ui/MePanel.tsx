import { useTranslations } from "@/i18n";
import { useAppStore } from "@/state/appStore";
import { useUiStore, type SettingsView } from "@/state/uiStore";
import { useChatProfile } from "@/hooks/useChatProfile";
import { useWallet } from "@/hooks/useWallet";
import { config, signingPinBackupActive } from "@/config";
import { WeChatNavBar } from "@/ui/WeChatNavBar";
import { ProfileAvatar } from "@/ui/ProfileAvatar";
import { shortAddress } from "@/wallet/address";

const ROWS: { view: SettingsView; icon: string; key: string }[] = [
  { view: "privacy", icon: "🔒", key: "privacy" },
  { view: "notifications", icon: "🔔", key: "notifications" },
  { view: "data", icon: "💾", key: "data" },
  { view: "about", icon: "ℹ️", key: "about" },
];

// EN: Me tab — WeChat 「我的」 profile card + settings rows.
// CN: 我的 Tab——微信「我的」资料卡 + 设置入口。
export function MePanel() {
  const t = useTranslations("me");
  const tApp = useTranslations("app");
  const tSettings = useTranslations("settings");
  const settingsView = useUiStore((s) => s.settingsView);
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const currentEntityId = useUiStore((s) => s.currentEntityId);
  const currentEntityName = useUiStore((s) => s.currentEntityName);
  const { account } = useAppStore();
  const { name, address, source, lock } = useWallet();
  const setPinsOpen = useAppStore((s) => s.setPinsOpen);
  const { profile } = useChatProfile(address, !config.useMock);

  const displayName = profile?.nickname ?? name ?? account?.nickname ?? tApp("defaultUser");
  const addr = address ?? account?.account;
  const entityHint =
    currentEntityName ??
    (currentEntityId != null ? t("entityId", { id: currentEntityId }) : t("selectEntity"));

  return (
    <aside className="tg-sidebar wx-panel wx-me-panel">
      <WeChatNavBar title={t("title")} />
      <button
        type="button"
        className="wx-me-card"
        onClick={() => setSettingsView("profile")}
      >
        <ProfileAvatar
          title={displayName}
          avatarCid={profile?.avatarCid}
          className="wx-me-avatar-wrap"
          size="sm"
        />
        <div className="wx-me-meta">
          <span className="wx-me-name">{displayName}</span>
          <span className="wx-me-sub">
            {t("onChainAccount", { address: addr ? shortAddress(addr) : "—" })}
          </span>
        </div>
        <span className="wx-cell-chevron">›</span>
      </button>

      <div className="wx-cell-group">
        {!config.useMock && (
          <button
            type="button"
            className={`wx-cell wx-cell-btn${settingsView === "entity" ? " active" : ""}`}
            onClick={() => setSettingsView("entity")}
          >
            <span className="wx-cell-icon">🏢</span>
            <span className="wx-cell-body">
              <span className="wx-cell-label">{t("entity")}</span>
              <span className="wx-cell-hint">{entityHint}</span>
            </span>
            <span className="wx-cell-chevron">›</span>
          </button>
        )}
        {!config.useMock && (
          <button
            type="button"
            className={`wx-cell wx-cell-btn${settingsView === "wallet" ? " active" : ""}`}
            onClick={() => setSettingsView("wallet")}
          >
            <span className="wx-cell-icon">👛</span>
            <span className="wx-cell-body">
              <span className="wx-cell-label">{t("wallet")}</span>
              <span className="wx-cell-hint">{t("walletHint")}</span>
            </span>
            <span className="wx-cell-chevron">›</span>
          </button>
        )}
        {!config.useMock && (
          <button
            type="button"
            className={`wx-cell wx-cell-btn${settingsView === "earnings" ? " active" : ""}`}
            onClick={() => setSettingsView("earnings")}
          >
            <span className="wx-cell-icon">💰</span>
            <span className="wx-cell-body">
              <span className="wx-cell-label">{t("earnings")}</span>
              <span className="wx-cell-hint">{t("earningsHint")}</span>
            </span>
            <span className="wx-cell-chevron">›</span>
          </button>
        )}
        {!config.useMock && (
          <button
            type="button"
            className={`wx-cell wx-cell-btn${settingsView === "staking" ? " active" : ""}`}
            onClick={() => setSettingsView("staking")}
          >
            <span className="wx-cell-icon">🛡️</span>
            <span className="wx-cell-body">
              <span className="wx-cell-label">{t("staking")}</span>
              <span className="wx-cell-hint">{t("stakingHint")}</span>
            </span>
            <span className="wx-cell-chevron">›</span>
          </button>
        )}
        {!config.useMock && (
          <button
            type="button"
            className={`wx-cell wx-cell-btn${settingsView === "chain" ? " active" : ""}`}
            onClick={() => setSettingsView("chain")}
          >
            <span className="wx-cell-icon">⛓️</span>
            <span className="wx-cell-body">
              <span className="wx-cell-label">{t("chain")}</span>
              <span className="wx-cell-hint">{t("chainHint")}</span>
            </span>
            <span className="wx-cell-chevron">›</span>
          </button>
        )}
        <button
          type="button"
          className={`wx-cell wx-cell-btn${settingsView === "language" ? " active" : ""}`}
          onClick={() => setSettingsView("language")}
        >
          <span className="wx-cell-icon">🌐</span>
          <span className="wx-cell-label">{tSettings("language")}</span>
          <span className="wx-cell-chevron">›</span>
        </button>
        {ROWS.map((r) => (
          <button
            key={r.view}
            type="button"
            className={`wx-cell wx-cell-btn${settingsView === r.view ? " active" : ""}`}
            onClick={() => setSettingsView(r.view)}
          >
            <span className="wx-cell-icon">{r.icon}</span>
            <span className="wx-cell-label">{t(r.key)}</span>
            <span className="wx-cell-chevron">›</span>
          </button>
        ))}
      </div>

      <div className="wx-cell-group">
        {signingPinBackupActive() && (
          <button
            type="button"
            className={`wx-cell wx-cell-btn${settingsView === "sendingKey" ? " active" : ""}`}
            onClick={() => setSettingsView("sendingKey")}
          >
            <span className="wx-cell-icon">🔑</span>
            <span className="wx-cell-body">
              <span className="wx-cell-label">{tSettings("signingPinTitle")}</span>
              <span className="wx-cell-hint">{tSettings("signingPinDesc")}</span>
            </span>
            <span className="wx-cell-chevron">›</span>
          </button>
        )}
        {!config.useMock && (
          <button type="button" className="wx-cell wx-cell-btn" onClick={() => setPinsOpen(true)}>
            <span className="wx-cell-icon">📌</span>
            <span className="wx-cell-label">{t("ipfsPin")}</span>
            <span className="wx-cell-chevron">›</span>
          </button>
        )}
        <button type="button" className="wx-cell wx-cell-btn wx-cell-danger" onClick={lock}>
          <span className="wx-cell-icon">🔒</span>
          <span className="wx-cell-label">{t("lockWallet")}</span>
          <span className="wx-cell-hint-inline">
            {source === "dev" ? t("devAccount") : t("desktopKeyring")}
          </span>
        </button>
      </div>
    </aside>
  );
}
