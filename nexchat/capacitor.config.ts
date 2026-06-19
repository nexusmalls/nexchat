// EN: Capacitor shell — Android loads remote CDN URL when CAP_SERVER_URL is set (OTA-friendly).
// CN: Capacitor 壳——设置 CAP_SERVER_URL 时 Android 加载远程 CDN（便于前端热更新）。

import type { CapacitorConfig } from "@capacitor/cli";

const defaultRemoteUrl = "https://nexusmall.net/nexchat/";
// EN: Set CAP_EMBEDDED=1 to bundle dist inside APK (offline / no OTA).
// CN: 设置 CAP_EMBEDDED=1 则 APK 内置 dist（离线、无 OTA）。
const embedded = process.env.CAP_EMBEDDED === "1";
const serverUrl = embedded ? undefined : (process.env.CAP_SERVER_URL?.trim() || defaultRemoteUrl);

const config: CapacitorConfig = {
  appId: "com.nexus.nexchat",
  appName: "NexChat",
  webDir: "dist",
  android: {
    allowMixedContent: true,
  },
  plugins: {
    SplashScreen: {
      launchAutoHide: true,
      androidSplashResourceName: "splash",
      androidScaleType: "CENTER_CROP",
      showSpinner: false,
      splashFullScreen: true,
      splashImmersive: true,
    },
    StatusBar: {
      style: "DARK",
      backgroundColor: "#ededed",
    },
  },
};

if (serverUrl) {
  const cleartext = serverUrl.startsWith("http://");
  config.server = {
    url: serverUrl.endsWith("/") ? serverUrl : `${serverUrl}/`,
    cleartext,
    androidScheme: cleartext ? "http" : "https",
  };
} else {
  // EN: Embedded dist build — still use https scheme for secure context (WebCrypto / WASM).
  // CN: 内置 dist 构建——仍用 https scheme 保证安全上下文（WebCrypto / WASM）。
  config.server = {
    androidScheme: "https",
  };
}

export default config;
