// EN: Copy text to system clipboard — Clipboard API with execCommand fallback for
// Capacitor Android WebView where async clipboard may fail silently.
// CN: 复制到系统剪贴板——优先 Clipboard API，Capacitor Android WebView 失败时用 execCommand 回退。

export async function copyToClipboard(text: string): Promise<boolean> {
  const value = text.trim();
  if (!value || typeof document === "undefined") return false;

  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(value);
      return true;
    } catch {
      /* fallback below */
    }
  }

  try {
    const el = document.createElement("textarea");
    el.value = value;
    el.setAttribute("readonly", "true");
    el.style.position = "fixed";
    el.style.left = "-9999px";
    el.style.top = "0";
    document.body.appendChild(el);
    el.focus();
    el.select();
    el.setSelectionRange(0, value.length);
    const ok = document.execCommand("copy");
    document.body.removeChild(el);
    return ok;
  } catch {
    return false;
  }
}
