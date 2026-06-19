// EN: Banner when the production WebSocket relay is required but disconnected — user can read
// local/archive history but must not send until reconnected. CN: 生产环境依赖 WS relay 但未连接时的
// 提示条——可读本地/归档，重连前不可发送。

import { useState } from "react";
import { useTranslations } from "@/i18n";
import { networkRelayRequired } from "@/relay/relayNetwork";
import { useAppStore } from "@/state/appStore";

export function RelayConnectBanner() {
  const t = useTranslations("app");
  const relayConnected = useAppStore((s) => s.relayConnected);
  const retryRelayConnect = useAppStore((s) => s.retryRelayConnect);
  const [retrying, setRetrying] = useState(false);

  if (!networkRelayRequired() || relayConnected) return null;

  return (
    <div className="tg-offchain-sync error" role="alert">
      <span>{t("relayDisconnected")}</span>
      <button
        type="button"
        className="tg-offchain-sync-btn"
        disabled={retrying}
        onClick={() => {
          setRetrying(true);
          void retryRelayConnect().finally(() => setRetrying(false));
        }}
      >
        {retrying ? t("relayRetrying") : t("relayRetry")}
      </button>
    </div>
  );
}
