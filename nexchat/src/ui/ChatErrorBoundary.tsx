import type { ReactNode } from "react";
import { useTranslations } from "@/i18n";
import { useAppStore } from "@/state/appStore";
import { ErrorBoundary } from "@/ui/ErrorBoundary";

/// EN: Conversation-scoped error boundary — reset returns to the chat list. CN: 会话级错误边界——重置回到
/// 会话列表。
export function ChatErrorBoundary({ children }: { children: ReactNode }) {
  const t = useTranslations("app");
  const closeConversation = useAppStore((s) => s.closeConversation);
  return (
    <ErrorBoundary
      title={t("crashTitle")}
      hint={t("crashHint")}
      reloadLabel={t("crashReload")}
      secondaryLabel={t("crashBack")}
      onSecondary={() => closeConversation()}
    >
      {children}
    </ErrorBoundary>
  );
}
