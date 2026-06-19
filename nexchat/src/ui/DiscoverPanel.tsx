import { useTranslations } from "@/i18n";
import { useUiStore } from "@/state/uiStore";
import { WeChatNavBar } from "@/ui/WeChatNavBar";

const ROWS = [
  { id: "market", icon: "📈", key: "market", enabled: true },
  { id: "prediction", icon: "📊", key: "prediction", enabled: true },
  { id: "shop", icon: "🛒", key: "shop", enabled: true },
  { id: "game", icon: "🎮", key: "game", enabled: false },
] as const;

// EN: Discover tab — WeChat 「发现」 page layout (placeholders for future features).
// CN: 发现 Tab——微信「发现」页布局（功能占位）。
export function DiscoverPanel() {
  const t = useTranslations("discover");
  const discoverView = useUiStore((s) => s.discoverView);
  const openMarket = useUiStore((s) => s.openMarket);
  const openPrediction = useUiStore((s) => s.openPrediction);
  const openShop = useUiStore((s) => s.openShop);

  return (
    <aside className="tg-sidebar wx-panel">
      <WeChatNavBar title={t("title")} />
      <div className="wx-cell-group">
        {ROWS.map((r) => (
          <button
            key={r.id}
            type="button"
            className={`wx-cell wx-cell-btn${
              (discoverView === "market" && r.id === "market") ||
              (discoverView === "prediction" && r.id === "prediction") ||
              (discoverView === "shop" && r.id === "shop")
                ? " active"
                : ""
            }`}
            disabled={!r.enabled}
            onClick={
              r.id === "market"
                ? openMarket
                : r.id === "prediction"
                  ? openPrediction
                  : r.id === "shop"
                    ? openShop
                    : undefined
            }
          >
            <span className="wx-cell-icon">{r.icon}</span>
            <span className="wx-cell-body">
              <span className="wx-cell-label">{t(r.key)}</span>
              <span className="wx-cell-hint">{t(`${r.key}Hint`)}</span>
            </span>
            <span className="wx-cell-chevron">›</span>
          </button>
        ))}
      </div>
      <p className="wx-panel-foot">{t("footer")}</p>
    </aside>
  );
}
