// EN: Telegram-style relative time for conversation list. CN: 会话列表用的 Telegram 风格相对时间。

export function formatChatTime(ts: number): string {
  if (!ts || ts <= 0) return "";
  const d = new Date(ts);
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  if (sameDay) {
    return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }
  const weekAgo = now.getTime() - 7 * 86400000;
  if (d.getTime() > weekAgo) {
    return d.toLocaleDateString(undefined, { weekday: "short" });
  }
  return d.toLocaleDateString(undefined, { month: "numeric", day: "numeric" });
}
