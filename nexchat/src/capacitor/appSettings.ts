// EN: Open native app settings / request Android microphone permission (Capacitor plugin).
// CN: 打开系统应用设置 / 请求 Android 麦克风权限（Capacitor 插件）。

import { Capacitor, registerPlugin } from "@capacitor/core";

interface AppSettingsPlugin {
  open(): Promise<void>;
  requestMicrophone(): Promise<{ granted: boolean }>;
}

const AppSettings = registerPlugin<AppSettingsPlugin>("AppSettings");

export function isMicrophonePermissionError(err: unknown): boolean {
  if (!(err instanceof Error)) return false;
  const msg = err.message.toLowerCase();
  return (
    msg.includes("permission") ||
    msg.includes("notallowed") ||
    msg.includes("not allowed") ||
    msg.includes("denied")
  );
}

/// EN: Open this app's permission page in system settings (Android/iOS shell). CN: 跳转系统里本应用的权限设置页。
export async function openAppSettings(): Promise<boolean> {
  if (!Capacitor.isNativePlatform()) return false;
  try {
    await AppSettings.open();
    return true;
  } catch {
    return false;
  }
}

/// EN: Android runtime RECORD_AUDIO before WebView getUserMedia. CN: WebView 录音前先申请 Android 麦克风权限。
export async function requestNativeMicrophone(): Promise<"granted" | "denied" | "unavailable"> {
  if (!Capacitor.isNativePlatform() || Capacitor.getPlatform() !== "android") return "granted";
  try {
    const { granted } = await AppSettings.requestMicrophone();
    return granted ? "granted" : "denied";
  } catch {
    // EN: Old APK without AppSettings plugin — rely on WebView getUserMedia prompt.
    // CN: 旧 APK 无 AppSettings 插件——交给 WebView getUserMedia 弹窗。
    return "unavailable";
  }
}
