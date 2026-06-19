import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

const pkg = JSON.parse(
  readFileSync(fileURLToPath(new URL("./package.json", import.meta.url)), "utf8"),
) as { version: string };

// EN: Single stamp per build — must match `version.json` and bundle `define`.
// CN: 每次构建唯一时间戳——须与 `version.json` 及 bundle 内 `define` 一致。
const appBuildStamp = {
  version: pkg.version,
  builtAt: new Date().toISOString(),
};

function writeVersionJsonPlugin() {
  return {
    name: "write-version-json",
    closeBundle() {
      writeFileSync("dist/version.json", `${JSON.stringify(appBuildStamp, null, 2)}\n`);
    },
  };
}

// EN: Vite config for the NexChat web client.
// CN: NexChat 网页客户端的 Vite 配置。
export default defineConfig({
  // EN: Relative base for Capacitor embedded builds (file:// / capacitor localhost).
  // CN: Capacitor 内置包需相对 base。
  base: process.env.CAPACITOR_BUILD === "1" ? "./" : "/nexchat/",
  define: {
    __NEXCHAT_APP_VERSION__: JSON.stringify(appBuildStamp.version),
    __NEXCHAT_APP_BUILT_AT__: JSON.stringify(appBuildStamp.builtAt),
  },
  plugins: [react(), writeVersionJsonPlugin()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
    dedupe: ["@polkadot/api", "@polkadot/util", "@polkadot/util-crypto"],
  },
  server: {
    port: 5173,
    // EN: Proxy local kubo so the browser can add/cat without CORS headers on :5001/:8080.
    // CN: 代理本地 kubo，浏览器无需在 :5001/:8080 开 CORS 即可 add/cat。
    proxy: {
      // EN: Avoid browser CORS when the local node is started without --rpc-cors=all.
      // CN: 本地节点未带 --rpc-cors=all 时，经 Vite 同源代理 chat_* JSON-RPC。
      "/chain-rpc": {
        target: "http://127.0.0.1:9944",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/chain-rpc\/?$/, "/"),
      },
      "/ipfs-api": {
        target: "http://127.0.0.1:5001",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/ipfs-api/, "/api/v0"),
      },
      "/ipfs-gateway": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/ipfs-gateway/, ""),
      },
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test/setupNodeWs.ts"],
  },
});
