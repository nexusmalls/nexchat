// EN: Native shell tweaks (status bar, safe area) when running inside Capacitor Android.
// CN: Capacitor Android 壳内原生微调（状态栏、安全区）。

import { Capacitor } from "@capacitor/core";
import { StatusBar, Style } from "@capacitor/status-bar";

function syncMobileShellClass(): void {
  if (typeof window === "undefined") return;
  const mobile =
    Capacitor.isNativePlatform() ||
    window.matchMedia("(max-width: 768px)").matches ||
    (window.visualViewport?.width ?? window.innerWidth) <= 768;
  document.documentElement.classList.toggle("mobile-shell", mobile);
  const w = document.documentElement.clientWidth || window.innerWidth;
  document.documentElement.style.setProperty("--app-width", `${w}px`);
}

// EN: Mark narrow viewports for mobile CSS (Huawei WebView / phone browser).
// CN: 为窄视口打上 mobile-shell 类（华为 WebView / 手机浏览器）。
export function initMobileShell(): void {
  if (typeof window === "undefined") return;
  syncMobileShellClass();
  window.addEventListener("resize", syncMobileShellClass);
  window.visualViewport?.addEventListener("resize", syncMobileShellClass);
  window.visualViewport?.addEventListener("scroll", syncMobileShellClass);
}

export async function initCapacitorShell(): Promise<void> {
  initMobileShell();
  if (!Capacitor.isNativePlatform()) return;
  document.documentElement.classList.add("capacitor-native");
  try {
    await StatusBar.setStyle({ style: Style.Dark });
    await StatusBar.setBackgroundColor({ color: "#ededed" });
  } catch {
    /* StatusBar unavailable */
  }
}
