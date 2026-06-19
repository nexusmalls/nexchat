import { config } from "@/config";
import { useTranslations } from "@/i18n";
import { useUiStore, type MainTab } from "@/state/uiStore";
import { useAppStore } from "@/state/appStore";
import { WxTabIcon } from "@/ui/WxTabIcon";

const TAB_IDS: MainTab[] = ["chats", "contacts", "discover", "me"];

// EN: WeChat-style bottom tab bar (4 tabs, green active state). CN: 微信风格底部四 Tab 导航。
export function BottomTabBar() {
  const t = useTranslations("nav");
  const mainTab = useUiStore((s) => s.mainTab);
  const setMainTab = useUiStore((s) => s.setMainTab);
  const badge = useAppStore((s) => s.badge);
  const contactRequestBadge = useAppStore((s) => s.contactRequestBadge);
  const groupInviteBadge = useAppStore((s) => s.groupInviteBadge);
  const contactsBadge = contactRequestBadge + groupInviteBadge;

  return (
    <nav className="wx-tabbar" aria-label={t("mainNav")}>
      {TAB_IDS.map((id) => {
        const active = mainTab === id;
        return (
          <button
            key={id}
            type="button"
            className={`wx-tabbar-btn${active ? " active" : ""}`}
            onClick={() => setMainTab(id)}
          >
            <span className="wx-tabbar-icon">
              <WxTabIcon kind={id} active={active} />
              {id === "chats" && badge > 0 && (
                <span className="wx-tabbar-badge">{badge > 99 ? "99+" : badge}</span>
              )}
              {id === "contacts" && contactsBadge > 0 && (
                <span className="wx-tabbar-badge">{contactsBadge > 99 ? "99+" : contactsBadge}</span>
              )}
            </span>
            <span className="wx-tabbar-label">{t(id)}</span>
          </button>
        );
      })}
      <span className="wx-tabbar-brand">{config.appName}</span>
    </nav>
  );
}
