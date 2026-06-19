import type { ReactNode } from "react";
import { config } from "@/config";
import { useWallet } from "@/hooks/useWallet";
import { shortAddress } from "@/wallet/address";

// EN: Shared sidebar header for chats / contacts / settings panels.
// CN: 会话 / 联系人 / 设置侧栏共用顶栏。
export function SidebarHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
}) {
  const { name, address, source } = useWallet();

  return (
    <header className="tg-sidebar-head">
      <div className="tg-sidebar-title">
        <span className="tg-app-name">{title}</span>
        {subtitle ? (
          <span className="tg-user-sub">{subtitle}</span>
        ) : (
          !config.useMock &&
          address && (
            <span className="tg-user-sub" title={address}>
              {source === "dev" ? "Dev" : name ?? "用户"} · {shortAddress(address)}
            </span>
          )
        )}
      </div>
      {actions && <div className="tg-sidebar-actions">{actions}</div>}
    </header>
  );
}
