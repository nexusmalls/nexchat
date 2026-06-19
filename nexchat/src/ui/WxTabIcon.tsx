// EN: Minimal SVG icons for WeChat-style bottom tabs. CN: 微信风格底部 Tab 图标。
export function WxTabIcon({ kind, active }: { kind: "chats" | "contacts" | "discover" | "me"; active: boolean }) {
  const color = active ? "var(--wx-green)" : "var(--wx-text-tertiary)";
  const stroke = color;
  const fill = active && kind === "chats" ? color : "none";

  if (kind === "chats") {
    return (
      <svg width="24" height="24" viewBox="0 0 24 24" aria-hidden>
        <path
          d="M8 10h8M8 14h5M4 6.5A8.5 8.5 0 0 1 20 12c0 4.2-3.4 7.5-7.6 7.5-.9 0-1.8-.2-2.6-.5L4 21l.8-3.2C4.3 16.6 4 15.3 4 14 4 9.6 4 6.5 4 6.5Z"
          stroke={stroke}
          strokeWidth="1.6"
          fill={fill}
          strokeLinejoin="round"
        />
      </svg>
    );
  }
  if (kind === "contacts") {
    return (
      <svg width="24" height="24" viewBox="0 0 24 24" aria-hidden>
        <circle cx="9" cy="9" r="3.2" stroke={stroke} strokeWidth="1.6" fill="none" />
        <path d="M4 19c0-2.8 2.2-5 5-5s5 2.2 5 5" stroke={stroke} strokeWidth="1.6" fill="none" />
        <path d="M16 8.5v6M19 11.5h-6" stroke={stroke} strokeWidth="1.6" strokeLinecap="round" />
      </svg>
    );
  }
  if (kind === "discover") {
    return (
      <svg width="24" height="24" viewBox="0 0 24 24" aria-hidden>
        <circle cx="12" cy="12" r="8" stroke={stroke} strokeWidth="1.6" fill="none" />
        <path d="M12 4v2M12 18v2M4 12h2M18 12h2" stroke={stroke} strokeWidth="1.6" strokeLinecap="round" />
        <circle cx="12" cy="12" r="2" fill={active ? color : "none"} stroke={stroke} strokeWidth="1.4" />
      </svg>
    );
  }
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" aria-hidden>
      <circle cx="12" cy="9" r="3.5" stroke={stroke} strokeWidth="1.6" fill={active ? color : "none"} />
      <path d="M5 20c0-3.3 3.1-6 7-6s7 2.7 7 6" stroke={stroke} strokeWidth="1.6" fill="none" />
    </svg>
  );
}
