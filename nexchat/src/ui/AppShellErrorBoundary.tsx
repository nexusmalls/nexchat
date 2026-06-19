import { useTranslations } from "@/i18n";
import { ErrorBoundary } from "@/ui/ErrorBoundary";

/// EN: Top-level shell error boundary — full page reload recovery. CN: 顶层外壳错误边界——整页刷新恢复。
export function AppShellErrorBoundary({ children }: { children: React.ReactNode }) {
  const t = useTranslations("app");
  return (
    <ErrorBoundary
      title={t("crashTitle")}
      hint={t("crashHint")}
      reloadLabel={t("crashReload")}
    >
      {children}
    </ErrorBoundary>
  );
}
