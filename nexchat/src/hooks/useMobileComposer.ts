// EN: True on mobile viewport or Capacitor native shell (voice composer target).
// CN: 手机视口或 Capacitor 原生壳时为 true（语音输入栏目标环境）。

import { Capacitor } from "@capacitor/core";
import { useEffect, useState } from "react";

function isMobileComposerEnv(): boolean {
  if (typeof window === "undefined") return false;
  try {
    if (Capacitor.isNativePlatform()) return true;
  } catch {
    /* Capacitor unavailable in tests */
  }
  return window.matchMedia("(max-width: 768px)").matches;
}

export function useMobileComposer(): boolean {
  const [mobile, setMobile] = useState(isMobileComposerEnv);

  useEffect(() => {
    const mq = window.matchMedia("(max-width: 768px)");
    const sync = () => setMobile(isMobileComposerEnv());
    sync();
    mq.addEventListener("change", sync);
    return () => mq.removeEventListener("change", sync);
  }, []);

  return mobile;
}
