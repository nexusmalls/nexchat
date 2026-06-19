// EN: Inline banner with a button to open system app permission settings.
// CN: 内联提示条，可跳转系统应用权限设置。

import { Capacitor } from "@capacitor/core";
import { openAppSettings } from "@/capacitor/appSettings";

interface PermissionPromptBarProps {
  message: string;
  actionLabel?: string;
  onDismiss?: () => void;
  /// EN: Custom primary action (skips default open settings). CN: 自定义主按钮（不打开系统设置）。
  onAction?: () => void;
  onBeforeOpenSettings?: () => void;
}

export function PermissionPromptBar({
  message,
  actionLabel = "去设置",
  onDismiss,
  onAction,
  onBeforeOpenSettings,
}: PermissionPromptBarProps) {
  const onPrimaryClick = () => {
    if (onAction) {
      onAction();
      return;
    }
    onBeforeOpenSettings?.();
    void (async () => {
      if (Capacitor.isNativePlatform()) {
        const ok = await openAppSettings();
        if (!ok) {
          window.alert(
            "请到系统设置开启麦克风：\n设置 → 应用 → NexChat → 权限 → 麦克风",
          );
        }
        return;
      }
      window.alert(
        "请在手机系统设置中允许麦克风：\n设置 → 应用 → NexChat（或浏览器）→ 权限 → 麦克风",
      );
    })();
  };

  return (
    <div className="wx-perm-prompt" role="alert">
      <span className="wx-perm-prompt-text">{message}</span>
      <button type="button" className="wx-perm-prompt-btn" onClick={onPrimaryClick}>
        {actionLabel}
      </button>
      {onDismiss && (
        <button type="button" className="wx-perm-prompt-dismiss" onClick={onDismiss} aria-label="关闭">
          ✕
        </button>
      )}
    </div>
  );
}
