// EN: Network relay requirement helpers (shared by relay transports + appStore). CN: 网络 relay 需求判定
// （relay 传输层与 appStore 共用）。

import { config } from "@/config";

/// EN: True when a production WebSocket relay URL is configured (mock/offline builds exempt). CN: 已配置
/// 生产 WS relay URL 时为 true（mock/离线构建除外）。
export function networkRelayRequired(): boolean {
  return !config.useMock && config.relayWs.length > 0;
}
