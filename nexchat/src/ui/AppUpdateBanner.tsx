// EN: Top banner when CDN `version.json` is newer than the running bundle.
// CN: CDN 有新版本时在顶部显示更新提示条。

interface AppUpdateBannerProps {
  visible: boolean;
  onRefresh: () => void;
}

export function AppUpdateBanner({ visible, onRefresh }: AppUpdateBannerProps) {
  if (!visible) return null;

  return (
    <div className="wx-app-update-banner" role="status">
      <span>发现新版本，请刷新以获取最新功能</span>
      <button type="button" className="wx-app-update-btn" onClick={onRefresh}>
        立即刷新
      </button>
    </div>
  );
}
