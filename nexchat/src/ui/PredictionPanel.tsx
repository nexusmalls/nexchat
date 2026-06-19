import { config } from "@/config";
import { useTranslations } from "@/i18n";
import { useUiStore } from "@/state/uiStore";

// EN: Discover prediction market hub (placeholder until chain module is wired).
// CN: 发现页预测市场入口（链上模块接入前的占位页）。
export function PredictionPanel() {
  const t = useTranslations("predictionHub");
  const setDiscoverView = useUiStore((s) => s.setDiscoverView);

  return (
    <main className="tg-main wx-market-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={() => setDiscoverView("list")}>
          ‹ {t("back")}
        </button>
        <span>{t("title")}</span>
        <span className="wx-market-refresh" aria-hidden="true" />
      </header>

      <div className="wx-market-scroll">
        {config.useMock && (
          <div className="wx-market-banner">
            {t("mockBanner")} <code>VITE_USE_MOCK=false</code>。
          </div>
        )}
        <section className="wx-market-card">
          <p className="wx-market-empty">{t("empty")}</p>
        </section>
        <p className="wx-market-foot">{t("footer")}</p>
      </div>
    </main>
  );
}
