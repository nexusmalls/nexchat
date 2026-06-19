import { useState, type ReactNode } from "react";
import { config, signingPinBackupActive } from "@/config";
import { useTranslations } from "@/i18n";
import { useAppStore } from "@/state/appStore";
import { SendingKeyPanel } from "@/ui/SendingKeyPanel";
import {
  getSyncAnchorTier,
  offchainSyncCoordinator,
  setSyncAnchorTier,
} from "@/store/offchainSyncCoordinator";
import { useUiStore, type SettingsView } from "@/state/uiStore";
import { ProfilePanel } from "@/ui/ProfilePanel";
import { ContactDetail } from "@/ui/ContactDetail";
import { ContactRequestDetail } from "@/ui/ContactRequestDetail";
import { GroupInviteDetail } from "@/ui/GroupInviteDetail";
import { GroupContactDetail } from "@/ui/GroupContactDetail";
import { ChatWindow } from "@/ui/ChatWindow";
import { ChatErrorBoundary } from "@/ui/ChatErrorBoundary";
import { MarketPanel } from "@/ui/MarketPanel";
import { ShopPanel } from "@/ui/ShopPanel";
import { PredictionPanel } from "@/ui/PredictionPanel";
import { EarningsMultiLevelPanel } from "@/ui/EarningsMultiLevelPanel";
import { EarningsPoolRewardPanel } from "@/ui/EarningsPoolRewardPanel";
import { EarningsSingleLinePanel } from "@/ui/EarningsSingleLinePanel";
import { EarningsPanel } from "@/ui/EarningsPanel";
import { EntityPanel } from "@/ui/EntityPanel";
import { WalletPanel } from "@/ui/WalletPanel";
import { StakingPanel } from "@/ui/StakingPanel";
import { ChainPanel } from "@/ui/ChainPanel";
import { LanguagePanel } from "@/ui/LanguagePanel";

type DetailSettingsView = Exclude<
  SettingsView,
  "list" | "profile" | "wallet" | "entity" | "earnings" | "earningsMultiLevel" | "earningsSingleLine" | "earningsPoolReward" | "staking" | "chain" | "language" | "sendingKey"
>;

// EN: Right/main column router for tabs and sub-views.
// CN: 右侧/主栏路由——Tab 与子视图。
export function MainPane() {
  const tContacts = useTranslations("contacts");
  const tDiscover = useTranslations("discover");
  const tMe = useTranslations("me");
  const mainTab = useUiStore((s) => s.mainTab);
  const discoverView = useUiStore((s) => s.discoverView);
  const settingsView = useUiStore((s) => s.settingsView);
  const selectedContact = useUiStore((s) => s.selectedContact);
  const selectedContactRequestId = useUiStore((s) => s.selectedContactRequestId);
  const selectedGroupInviteId = useUiStore((s) => s.selectedGroupInviteId);
  const selectedGroupConvId = useUiStore((s) => s.selectedGroupConvId);
  const setSettingsView = useUiStore((s) => s.setSettingsView);

  if (mainTab === "chats") {
    return (
      <ChatErrorBoundary>
        <ChatWindow />
      </ChatErrorBoundary>
    );
  }

  if (mainTab === "contacts") {
    if (selectedContactRequestId) {
      return <ContactRequestDetail reqId={selectedContactRequestId} />;
    }
    if (selectedGroupInviteId) {
      return <GroupInviteDetail inviteId={selectedGroupInviteId} />;
    }
    if (selectedGroupConvId) {
      return <GroupContactDetail convId={selectedGroupConvId} />;
    }
    if (selectedContact) return <ContactDetail address={selectedContact} />;
    return (
      <main className="tg-main tg-main-empty">
        <div className="tg-empty-state">
          <div className="tg-empty-icon">👥</div>
          <h2>{tContacts("emptyTitle")}</h2>
          <p>{tContacts("emptyHint")}</p>
        </div>
      </main>
    );
  }

  if (mainTab === "me") {
    if (settingsView === "profile") return <ProfilePanel />;
    if (settingsView === "wallet") return <WalletPanel />;
    if (settingsView === "entity") return <EntityPanel />;
    if (settingsView === "earnings") return <EarningsPanel />;
    if (settingsView === "earningsMultiLevel") return <EarningsMultiLevelPanel />;
    if (settingsView === "earningsSingleLine") return <EarningsSingleLinePanel />;
    if (settingsView === "earningsPoolReward") return <EarningsPoolRewardPanel />;
    if (settingsView === "staking") return <StakingPanel />;
    if (settingsView === "chain") return <ChainPanel />;
    if (settingsView === "language") return <LanguagePanel />;
    if (settingsView === "sendingKey") {
      if (signingPinBackupActive()) return <SendingKeyPanel />;
    } else if (settingsView !== "list") {
      return <SettingsDetail view={settingsView} onBack={() => setSettingsView("list")} />;
    }
  }

  if (mainTab === "discover") {
    if (discoverView === "market") return <MarketPanel />;
    if (discoverView === "prediction") return <PredictionPanel />;
    if (discoverView === "shop") return <ShopPanel />;
    return (
      <main className="tg-main tg-main-empty wx-main-hint">
        <div className="tg-empty-state">
          <div className="tg-empty-icon">🧭</div>
          <h2>{tDiscover("emptyTitle")}</h2>
          <p>{tDiscover("emptyHint")}</p>
        </div>
      </main>
    );
  }

  if (mainTab === "me") {
    return (
      <main className="tg-main tg-main-empty wx-main-hint">
        <div className="tg-empty-state">
          <div className="tg-empty-icon">👤</div>
          <h2>{tMe("emptyTitle")}</h2>
          <p>{tMe("emptyHint")}</p>
        </div>
      </main>
    );
  }

  return null;
}

function SettingsDetail({
  view,
  onBack,
}: {
  view: DetailSettingsView;
  onBack: () => void;
}) {
  const t = useTranslations("settings");
  const tc = useTranslations("common");

  const titles: Record<DetailSettingsView, string> = {
    privacy: t("privacy"),
    notifications: t("notifications"),
    data: t("data"),
    about: t("about"),
  };

  const content: Record<DetailSettingsView, ReactNode> = {
    privacy: (
      <>
        <SettingToggle label={t("privacyE2ee")} value={config.mlsEnabled} readonly />
        <SettingToggle label={t("privacyDeliveryTokens")} value={config.deliveryTokensEnabled} readonly />
        <SettingToggle label={t("privacyConvIndex")} value={config.convIndexEnabled} readonly />
        <SettingToggle label={t("privacyContactsVault")} value={config.contactsVaultEnabled} readonly />
        <SettingToggle label={t("privacyMsgArchive")} value={config.msgArchiveEnabled} readonly />
        <p className="tg-settings-note">{t("privacyNote")}</p>
        <p className="tg-settings-note">{t("privacyDeleteRecallNote")}</p>
      </>
    ),
    notifications: (
      <>
        <SettingToggle label={t("notifBadge")} value defaultChecked readonly />
        <SettingToggle label={t("notifMention")} value defaultChecked readonly />
        <p className="tg-settings-note">{t("notifNote")}</p>
      </>
    ),
    data: (
      <>
        <SettingToggle label={t("dataIpfs")} value={config.ipfsEnabled} readonly />
        <SettingToggle label={t("dataPin")} value={config.ipfsPinEnabled} readonly />
        <ChainAnchorToggle />
        <div className="tg-profile-row">
          <span className="tg-profile-row-label">{t("dataChunkThreshold")}</span>
          <span className="tg-profile-row-value">
            {tc("mb", { size: (config.ipfsChunkThreshold / 1024 / 1024).toFixed(0) })}
          </span>
        </div>
        <p className="tg-settings-note">{t("dataNote")}</p>
      </>
    ),
    about: (
      <>
        <div className="tg-about-logo">N</div>
        <h2 className="tg-about-name">{config.appName}</h2>
        <p className="tg-about-ver">{t("aboutVersion")}</p>
        <a className="tg-about-download" href="/nexchat/download.html" target="_blank" rel="noopener noreferrer">
          {t("downloadApp")}
        </a>
        <ul className="tg-about-list">
          <li>{t("aboutFeature1")}</li>
          <li>{t("aboutFeature2")}</li>
          <li>{t("aboutFeature3")}</li>
          <li>{t("aboutFeature4")}</li>
        </ul>
      </>
    ),
  };

  return (
    <main className="tg-main tg-settings-main">
      <header className="tg-sub-head">
        <button type="button" className="tg-sub-back" onClick={onBack}>
          {t("back")}
        </button>
        <span>{titles[view]}</span>
      </header>
      <div className="tg-settings-detail">{content[view]}</div>
    </main>
  );
}

// EN: "On-chain encrypted backup anchor" tier switch (ADR §7: Standard ↔ Relay-only),
// persisted per account; the privacy note discloses the §5.7 on-chain visible surface
// (anchor activity / cadence / ciphertext length / fee payer) and the §5.8 operator
// Crust CID exposure. Turning it on nudges an immediate chain flush.
// CN: 「链上加密备份锚」档位开关（ADR §7：Standard ↔ Relay-only），按账户持久化；隐私
// 文案披露 §5.7 链上可见面（锚活跃度/更新节奏/密文长度/付费账户）与 §5.8 运营者 Crust
// CID 暴露。开启时立即触发一次链上 flush。
function ChainAnchorToggle() {
  const t = useTranslations("settings");
  const account = useAppStore((s) => s.account?.account ?? "");
  const [tier, setTier] = useState(() => (account ? getSyncAnchorTier(account) : "standard"));

  if (!account || config.useMock) return null;
  const checked = tier === "standard";
  return (
    <>
      <label className="tg-setting-toggle">
        <span>{t("dataChainAnchor")}</span>
        <input
          type="checkbox"
          checked={checked}
          onChange={(e) => {
            const next = e.target.checked ? "standard" : "relay_only";
            setSyncAnchorTier(account, next);
            setTier(next);
            if (next === "standard") void offchainSyncCoordinator.flushChain({ force: true });
          }}
        />
      </label>
      <p className="tg-settings-note">{t("dataChainAnchorNote")}</p>
    </>
  );
}

function SettingToggle({
  label,
  value,
  defaultChecked,
  readonly,
}: {
  label: string;
  value?: boolean;
  defaultChecked?: boolean;
  readonly?: boolean;
}) {
  const checked = value ?? defaultChecked ?? false;
  return (
    <label className="tg-setting-toggle">
      <span>{label}</span>
      <input type="checkbox" checked={checked} readOnly={readonly} disabled={readonly} />
    </label>
  );
}
