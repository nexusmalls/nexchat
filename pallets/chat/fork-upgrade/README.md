# Fork 升级演练工具 / Chopsticks fork upgrade helpers

用于在 **Chopsticks fork**（`ws://127.0.0.1:8000`）上提交 **spec 103** `set_code` 并验证。

## 为什么 `npx @polkadot/apps@latest` 报错？

`@polkadot/apps` **没有 CLI 可执行文件**，npm 会报 `could not determine executable to run`。
Polkadot.js Apps 是前端网站，需要 **clone 后 yarn start**，或用 **Node 脚本**（本目录）。

## 1. 启动 Chopsticks（终端 A）

```bash
cd /home/xiaodong/文档/nexus
npx @acala-network/chopsticks@latest -c chopsticks-fork.yml
```

## 2. 验证基线（终端 B，应为 102）

```bash
cd pallets/chat/fork-upgrade
bash verify-v103.sh
```

## 3. 提交 set_code（需 Sudo 助记词）

```bash
cd pallets/chat/fork-upgrade
npm install

# 主网 fork 上的 sudo 账户助记词（Developer → Chain state → sudo → key 查地址）
SUDO_URI='你的 sudo 助记词...' npm run set-code
```

## 4. 再次验证（应为 103 + MsgIdentity）

```bash
npm run verify
```

## 可选：本地 Polkadot.js Apps（图形界面）

```bash
git clone --depth 1 https://github.com/polkadot-js/apps.git ~/文档/polkadotapps
cd ~/文档/polkadotapps
yarn install   # 需要 Yarn；无则用 corepack enable
yarn start     # 打开 http://localhost:3000 ，连 ws://127.0.0.1:8000
```

不要用 `https://polkadot.js.org` 连 `ws://`（浏览器会报 SecurityError）。
