import type { ReactNode } from "react";

// EN: WeChat-style top navigation bar (centered title, optional back + right actions).
// CN: 微信风格顶栏（居中标题，可选返回与右侧操作）。
export function WeChatNavBar({
  title,
  actions,
  onBack,
}: {
  title: string;
  actions?: ReactNode;
  onBack?: () => void;
}) {
  return (
    <header className="wx-navbar">
      <div className="wx-navbar-side wx-navbar-left">
        {onBack ? (
          <button type="button" className="wx-nav-back" onClick={onBack} aria-label="返回">
            ‹
          </button>
        ) : null}
      </div>
      <h1 className="wx-navbar-title">{title}</h1>
      <div className="wx-navbar-side wx-navbar-right">{actions}</div>
    </header>
  );
}
