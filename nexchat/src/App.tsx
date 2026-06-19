import { useEffect } from "react";
import { useTranslations } from "@/i18n";
import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { config } from "@/config";
import { ConversationList } from "@/ui/ConversationList";
import { ContactsPanel } from "@/ui/ContactsPanel";
import { DiscoverPanel } from "@/ui/DiscoverPanel";
import { MePanel } from "@/ui/MePanel";
import { MainPane } from "@/ui/MainPane";
import { BottomTabBar } from "@/ui/BottomTabBar";
import { PinPanel } from "@/ui/PinPanel";
import { NewDirectChat } from "@/ui/NewDirectChat";
import { NewGroupChat } from "@/ui/NewGroupChat";
import { JoinGroupById } from "@/ui/JoinGroupById";
import { GroupManageModal } from "@/ui/GroupManageModal";
import { InviteGroupMembers } from "@/ui/InviteGroupMembers";
import { WalletGate } from "@/ui/WalletGate";
import { ShareToChatModal } from "@/ui/ShareToChatModal";
import { ForwardMessageModal } from "@/ui/ForwardMessageModal";
import { AppUpdateBanner } from "@/ui/AppUpdateBanner";
import { OffchainSyncBanner } from "@/ui/OffchainSyncBanner";
import { RelayConnectBanner } from "@/ui/RelayConnectBanner";
import { SendingAuthorityBanner } from "@/ui/SendingAuthorityBanner";
import { useAppVersionCheck } from "@/hooks/useAppVersionCheck";

// EN: Telegram shell — tabbed sidebar + main pane + bottom navigation.
// CN: Telegram 外壳——Tab 侧栏 + 主栏 + 底部导航。
export function App() {
  const t = useTranslations("app");
  const { account, unlock, loading, error } = useAppStore();
  const notice = useAppStore((s) => s.notice);
  const mainTab = useUiStore((s) => s.mainTab);
  const discoverView = useUiStore((s) => s.discoverView);
  const settingsView = useUiStore((s) => s.settingsView);
  const selectedContact = useUiStore((s) => s.selectedContact);
  const selectedContactRequestId = useUiStore((s) => s.selectedContactRequestId);
  const selectedGroupInviteId = useUiStore((s) => s.selectedGroupInviteId);
  const selectedGroupConvId = useUiStore((s) => s.selectedGroupConvId);
  const activeConvId = useAppStore((s) => s.activeConvId);
  const mobileChatOpen = mainTab === "chats" && !!activeConvId;
  const discoverFeatureOpen = mainTab === "discover" && discoverView !== "list";
  const meFeatureOpen = mainTab === "me" && settingsView !== "list";
  const contactsFeatureOpen =
    mainTab === "contacts" &&
    (selectedContact != null ||
      selectedContactRequestId != null ||
      selectedGroupInviteId != null ||
      selectedGroupConvId != null);
  const { updateAvailable, applyUpdate } = useAppVersionCheck(import.meta.env.PROD);

  useEffect(() => {
    if (!account && config.useMock) {
      const tag = Math.floor(Math.random() * 1000);
      void unlock(`5User-${tag}`, `用户${tag}`);
    }
  }, [account, unlock]);

  if (!config.useMock && !account) {
    return (
      <>
        <AppUpdateBanner visible={updateAvailable} onRefresh={applyUpdate} />
        <WalletGate />
      </>
    );
  }

  return (
    <div className="tg-app">
      <AppUpdateBanner visible={updateAvailable} onRefresh={applyUpdate} />
      <RelayConnectBanner />
      <OffchainSyncBanner />
      <SendingAuthorityBanner />
      {loading && !account && <div className="tg-toast">{t("initSession")}</div>}
      {error && <div className="tg-toast tg-toast-err">{error}</div>}
      {notice && <div className="tg-toast tg-toast-ok">{notice}</div>}
      <PinPanel />
      <NewDirectChat />
      <NewGroupChat />
      <JoinGroupById />
      <GroupManageModal />
      <InviteGroupMembers />
      <ShareToChatModal />
      <ForwardMessageModal />
      <div
        className={`tg-app-body${mobileChatOpen ? " wx-mobile-chat" : ""}${discoverFeatureOpen ? " wx-discover-feature" : ""}${meFeatureOpen ? " wx-me-feature" : ""}${contactsFeatureOpen ? " wx-contacts-feature" : ""}`}
      >
        {mainTab === "chats" && !mobileChatOpen && <ConversationList />}
        {mainTab === "contacts" && !contactsFeatureOpen && <ContactsPanel />}
        {mainTab === "discover" && !discoverFeatureOpen && <DiscoverPanel />}
        {mainTab === "me" && !meFeatureOpen && <MePanel />}
        <MainPane />
      </div>
      {!mobileChatOpen && <BottomTabBar />}
    </div>
  );
}
