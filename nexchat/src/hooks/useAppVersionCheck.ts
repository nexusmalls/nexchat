import { App } from "@capacitor/app";
import { Capacitor } from "@capacitor/core";
import { useCallback, useEffect, useState } from "react";
import { checkForAppUpdate, reloadApp } from "@/capacitor/versionCheck";

const POLL_MS = 5 * 60_000;

// EN: Poll `version.json` in production; prompt when CDN has a newer build.
// CN: 生产环境轮询 `version.json`；CDN 有新构建时提示刷新。
export function useAppVersionCheck(enabled: boolean) {
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const baseUrl = import.meta.env.BASE_URL;

  const runCheck = useCallback(async () => {
    if (!enabled) return;
    const hasUpdate = await checkForAppUpdate(baseUrl);
    setUpdateAvailable(hasUpdate);
  }, [enabled, baseUrl]);

  useEffect(() => {
    if (!enabled) {
      setUpdateAvailable(false);
      return;
    }

    void runCheck();
    const timer = window.setInterval(() => void runCheck(), POLL_MS);

    let removeHandle: (() => void) | undefined;
    if (Capacitor.isNativePlatform()) {
      void App.addListener("appStateChange", ({ isActive }) => {
        if (isActive) void runCheck();
      }).then((handle) => {
        removeHandle = () => void handle.remove();
      });
    } else {
      const onVisible = () => {
        if (document.visibilityState === "visible") void runCheck();
      };
      document.addEventListener("visibilitychange", onVisible);
      removeHandle = () => document.removeEventListener("visibilitychange", onVisible);
    }

    return () => {
      window.clearInterval(timer);
      removeHandle?.();
    };
  }, [enabled, runCheck]);

  return {
    updateAvailable,
    applyUpdate: reloadApp,
  };
}
