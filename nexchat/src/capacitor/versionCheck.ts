// EN: Poll CDN `version.json` and detect newer frontend builds (Capacitor remote URL / web).
// CN: 轮询 CDN `version.json`，检测是否有新前端构建（Capacitor 远程 URL / 网页）。

export interface AppVersionInfo {
  version: string;
  builtAt: string;
}

// EN: Version baked into the running JS bundle at build time (not sessionStorage).
// CN: 构建时写入当前 JS bundle 的版本（不用 sessionStorage）。
export function getRunningAppVersion(): AppVersionInfo {
  return {
    version: typeof __NEXCHAT_APP_VERSION__ !== "undefined" ? __NEXCHAT_APP_VERSION__ : "0.0.0-dev",
    builtAt: typeof __NEXCHAT_APP_BUILT_AT__ !== "undefined" ? __NEXCHAT_APP_BUILT_AT__ : "",
  };
}

// EN: True when remote build differs from the currently running bundle.
// CN: 远程构建与当前运行的 bundle 不一致时为 true。
export function isNewerAppVersion(
  running: AppVersionInfo | null,
  remote: AppVersionInfo,
): boolean {
  if (!running?.builtAt) return false;
  if (remote.version !== running.version) return true;
  return remote.builtAt !== running.builtAt;
}

export async function fetchRemoteAppVersion(baseUrl: string): Promise<AppVersionInfo | null> {
  const root = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  const url = `${root}version.json?t=${Date.now()}`;
  try {
    const res = await fetch(url, { cache: "no-store" });
    if (!res.ok) return null;
    const data = (await res.json()) as Partial<AppVersionInfo>;
    if (!data.version || !data.builtAt) return null;
    return { version: String(data.version), builtAt: String(data.builtAt) };
  } catch {
    return null;
  }
}

export async function checkForAppUpdate(baseUrl: string): Promise<boolean> {
  const remote = await fetchRemoteAppVersion(baseUrl);
  if (!remote) return false;
  return isNewerAppVersion(getRunningAppVersion(), remote);
}

// EN: Hard reload with cache-bust so the browser picks up the latest bundle.
// CN: 带 cache-bust 的硬刷新，确保浏览器加载最新 bundle。
export function reloadApp(): void {
  if (typeof window === "undefined") return;
  const url = new URL(window.location.href);
  url.searchParams.set("_v", String(Date.now()));
  window.location.replace(url.toString());
}

const CHUNK_RELOAD_KEY = "nexchat:chunk-reload";

function isStaleChunkError(message: string): boolean {
  return (
    message.includes("dynamically imported module") ||
    message.includes("Failed to fetch") ||
    message.includes("Importing a module script failed")
  );
}

/// EN: One-shot auto reload when a lazy chunk 404s after deploy (stale WebView cache).
/// CN: 部署后 lazy chunk 404（WebView 缓存旧 index）时自动刷新一次。
export function installChunkLoadRecovery(): void {
  if (typeof window === "undefined" || !import.meta.env.PROD) return;

  const tryReloadOnce = (): boolean => {
    if (sessionStorage.getItem(CHUNK_RELOAD_KEY)) return false;
    sessionStorage.setItem(CHUNK_RELOAD_KEY, "1");
    reloadApp();
    return true;
  };

  window.addEventListener("vite:preloadError", (ev) => {
    ev.preventDefault();
    tryReloadOnce();
  });

  window.addEventListener("unhandledrejection", (ev) => {
    const msg = String((ev.reason as Error | undefined)?.message ?? ev.reason ?? "");
    if (!isStaleChunkError(msg)) return;
    if (tryReloadOnce()) ev.preventDefault();
  });

  window.addEventListener(
    "error",
    (ev) => {
      const msg = String(ev.message ?? "");
      if (!isStaleChunkError(msg)) return;
      tryReloadOnce();
    },
    true,
  );
}

export function clearChunkReloadGuard(): void {
  if (typeof window === "undefined") return;
  sessionStorage.removeItem(CHUNK_RELOAD_KEY);
}

/// EN: Before React mounts, compare CDN `version.json` to the running bundle; reload if stale.
/// CN: React 挂载前比对 CDN `version.json` 与当前 bundle；过期则刷新。
export async function ensureFreshBundle(baseUrl: string): Promise<void> {
  if (!import.meta.env.PROD) return;
  const remote = await fetchRemoteAppVersion(baseUrl);
  if (!remote) return;
  if (!isNewerAppVersion(getRunningAppVersion(), remote)) return;
  reloadApp();
  await new Promise<void>(() => {
    /* navigation in progress */
  });
}
