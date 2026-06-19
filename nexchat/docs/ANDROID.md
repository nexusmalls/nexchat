# NexChat Android（Capacitor）

EN: Thin native shell + Vite web UI. **Remote URL mode** (recommended) loads your CDN on each launch — frontend updates without republishing APK.

CN: 原生薄壳 + Vite 网页。推荐 **远程 URL 模式**：每次启动加载 CDN，前端可热更新而无需重发 APK。

## 前置条件

- Node.js 18+
- [Android Studio](https://developer.android.com/studio)（含 SDK、JDK 17）
- 环境变量 `ANDROID_HOME` 或 Android Studio 默认 SDK 路径

## 两种运行模式

| 模式 | 何时用 | 前端更新 |
|------|--------|----------|
| **远程 URL**（推荐） | 生产 / 测试服 | 只部署 `dist` 到 CDN，用户重启 App 即新版本 |
| **内置 dist** | 离线演示、内网无外网 | 改前端后需 `cap sync` + 重打 APK |

### 1. 远程 URL（推荐，便于 OTA）

```bash
cd nexchat

# 1) 先把前端部署到 HTTPS（示例）
#    npm run build && rsync dist/ user@cdn:/var/www/nexchat/

# 2) 把壳指向该地址并同步进 Android 工程（默认已是生产 URL）
npm run android:sync:remote
# 或手动:
# export CAP_SERVER_URL=https://nexusmall.net/nexchat/
# CAP_SERVER_URL=$CAP_SERVER_URL npx cap sync android

# 3) 打 debug 包（或 Android Studio → Build APK）
npm run android:apk
# 输出: android/app/build/outputs/apk/debug/app-debug.apk
```

**局域网联调**（手机访问电脑上的 Vite）：

```bash
# 电脑先: npm run dev -- --host 0.0.0.0
# 查电脑局域网 IP，例如 192.168.1.10
export CAP_SERVER_URL=http://192.168.1.10:5173
CAP_SERVER_URL=$CAP_SERVER_URL npx cap sync android
npx cap open android
```

模拟器访问本机 Vite：`CAP_SERVER_URL=http://10.0.2.2:5173`

> 远程模式下，链节点 / relay 地址由 **网页里的 `.env` 构建进 dist** 决定，改 `VITE_WS_ENDPOINT` 等后需重新 `npm run build` 并部署 CDN，**不必**重打 APK。

### 2. 内置 dist（离线包）

```bash
npm run cap:sync          # build:cap + cap sync
npm run android:apk
```

未设置 `CAP_SERVER_URL` 时，APK 内嵌 `dist/`，WebView 走 Capacitor `https` localhost scheme。

## 日常命令

```bash
npm run cap:sync      # 重新构建 web 并同步到 android/
npm run cap:open      # 用 Android Studio 打开工程
npm run android:apk   # 命令行打 debug APK
```

## 发布 release

在 Android Studio：`Build → Generate Signed Bundle / APK`，或使用 `assembleRelease` + 自有签名配置。

Release 仍建议使用 **HTTPS 的 `CAP_SERVER_URL`**，壳极少改动，业务迭代走 CDN。

## 版本提示（可选）

每次 `npm run build` 会在 `dist/version.json` 写入版本号与构建时间：

```json
{ "version": "0.1.0", "builtAt": "2026-06-09T12:00:00Z" }
```

App 启动后每 5 分钟轮询一次（回到前台也会检查）。若 CDN 上的 `version.json` 与当前会话记录不一致，顶部会出现 **「发现新版本，请刷新」** 条，点击 **立即刷新** 即可加载最新前端。

> 远程 URL 模式下只需部署 `dist/`（含 `version.json`），**不必**重打 APK。内置 dist 模式（`CAP_EMBEDDED=1`）改前端后仍需 `cap sync` + 重打 APK。

## 用户下载页

生产环境提供独立下载页与 APK：

| 资源 | URL |
|------|-----|
| 下载页 | `https://nexusmall.net/nexchat/download.html` |
| APK | `https://nexusmall.net/nexchat/nexchat.apk` |

本地构建并上传：

```bash
cd nexchat
npm run build -- --base=/nexchat/
npm run android:release:remote   # 生成 debug APK

# 部署网页（含 download.html）
cp deploy/deploy-web.env.example deploy/deploy-web.env   # 首次：编辑 SSH 等
export SSHPASS='…'   # 或使用 deploy-web.env 里的 DEPLOY_SSHPASS
npm run deploy:web:remote

# 等价于手动 rsync（脚本会排除 nexchat.apk）：
# SSHPASS='…' sshpass -e rsync -avz --delete --exclude nexchat.apk dist/ root@151.158.134.181:/var/www/nexchat/

# 单独上传 APK（避免 rsync --delete 误删）
SSHPASS='…' sshpass -e rsync -avz \
  android/app/build/outputs/apk/debug/app-debug.apk \
  root@151.158.134.181:/var/www/nexchat/nexchat.apk
```

App 内 **我的 → 关于** 也有「下载 Android App」入口，指向上述下载页。

## 工程说明

- `capacitor.config.ts` — 默认 `CAP_SERVER_URL=https://nexusmall.net/nexchat/`；`CAP_EMBEDDED=1` 切回内置包
- `src/capacitor/versionCheck.ts` — 轮询与版本比对
- `android/` — Capacitor Android 工程（可入库）
- `CAPACITOR_BUILD=1` — Vite `base: './'`，适配内置包资源路径
- 明文 HTTP — 仅用于开发联调（`network_security_config.xml`）；生产请用 HTTPS

## 常见问题

**白屏** — 检查 `CAP_SERVER_URL` 是否可从手机访问；HTTPS 证书是否有效。

**连不上本地节点** — 手机不能使用 `127.0.0.1` 指你的电脑；改用局域网 IP 或公网节点，并在构建 dist 时写好 `VITE_WS_ENDPOINT` / `VITE_HTTP_ENDPOINT`。

**WASM / 钱包** — 需要 Android 7+（`minSdk 23`）；WebCrypto 在 WebView 中可用。
