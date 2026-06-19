// EN: Telegram-inspired welcome / wallet gate (login, create, import).
// CN: Telegram 风格欢迎页 / 钱包门控（登录、创建、导入）。

import { useCallback, useEffect, useState } from "react";
import { config } from "@/config";
import { useWallet } from "@/hooks/useWallet";
import { useWalletStore } from "@/state/walletStore";
import { shortAddress } from "@/wallet/address";
import { type DesktopAccount } from "@/wallet/desktopKeyring";
import { validateWalletPassword } from "@/wallet/passwordValidation";
import { reloadApp } from "@/capacitor/versionCheck";
import {
  CreateWalletFlow,
  type CreateWalletStep,
} from "@/ui/wallet/CreateWalletFlow";

type Screen = "welcome" | "unlock" | "create" | "import";

const CREATE_TITLES: Record<CreateWalletStep, string> = {
  1: "创建账户",
  2: "备份助记词",
  3: "验证助记词",
};

function isLocalDevHost(): boolean {
  if (typeof window === "undefined") return false;
  const h = window.location.hostname;
  return h === "localhost" || h === "127.0.0.1";
}

function isStaleBundleError(message: string): boolean {
  return (
    message.includes("dynamically imported module") ||
    message.includes("Failed to fetch") ||
    message.includes("Importing a module script failed")
  );
}

function errorHintFor(message: string): React.ReactNode {
  if (isStaleBundleError(message)) {
    return (
      <>
        <br />
        <span className="wallet-error-hint">
          页面资源版本不匹配（部署后 WebView 缓存了旧文件）。
          手机请点下方「立即刷新」；浏览器可用 Ctrl+Shift+R 或清除站点缓存。
        </span>
        <br />
        <button type="button" className="tg-handoff-btn wallet-error-refresh" onClick={reloadApp}>
          立即刷新
        </button>
      </>
    );
  }
  if (!isLocalDevHost()) return null;
  return (
    <>
      <br />
      <span className="wallet-error-hint">
        本地开发需同时运行：<code>nexus-node --dev</code> 与 <code>npm run relay:server</code>
      </span>
    </>
  );
}

export function WalletGate() {
  const {
    loadAccounts,
    unlockAndEnter,
    enterWithDev,
    importAndEnter,
    removeAccount,
  } = useWallet();

  const [screen, setScreen] = useState<Screen>("welcome");
  const [accounts, setAccounts] = useState<DesktopAccount[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [createStep, setCreateStep] = useState<CreateWalletStep>(1);
  const [createFlowKey, setCreateFlowKey] = useState(0);
  const [importMnemonic, setImportMnemonic] = useState("");
  const [importName, setImportName] = useState("");
  const [importPassword, setImportPassword] = useState("");
  const [importConfirm, setImportConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const persistedAddress = useWalletStore((s) => s.address);

  const refresh = useCallback(async () => {
    const list = await loadAccounts();
    setAccounts(list);
    if (list.length === 0) return;
    if (selected) return;
    const preferred = persistedAddress
      ? list.find((a) => a.address === persistedAddress)?.address
      : null;
    setSelected(preferred ?? list[0]!.address);
  }, [loadAccounts, selected, persistedAddress]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function run(fn: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      const accountsNow = await loadAccounts();
      if (accountsNow.length > 0 && screen === "create") {
        setError(`${msg}（账户已保存在本地，可到「使用已有账户」解锁）`);
      } else {
        setError(msg);
      }
    } finally {
      setBusy(false);
    }
  }

  function goWelcome() {
    setScreen("welcome");
    setError(null);
    setCreateStep(1);
    setCreateFlowKey((k) => k + 1);
  }

  function openCreate() {
    setCreateStep(1);
    setCreateFlowKey((k) => k + 1);
    setScreen("create");
  }

  if (screen === "welcome") {
    return (
      <div className="tg-welcome">
        <div className="tg-welcome-inner">
          <div className="tg-welcome-logo" aria-hidden>
            {config.appName[0]?.toUpperCase() ?? "N"}
          </div>
          <h1 className="tg-welcome-title">{config.appName}</h1>
          <p className="tg-welcome-sub">端到端加密 · 去中心化社交聊天</p>

          {accounts.length > 0 ? (
            <button
              type="button"
              className="tg-welcome-primary"
              onClick={() => setScreen("unlock")}
            >
              使用已有账户
            </button>
          ) : (
            <button type="button" className="tg-welcome-primary" onClick={openCreate}>
              开始使用
            </button>
          )}

          <div className="tg-welcome-links">
            <button type="button" onClick={openCreate}>
              创建新账户
            </button>
            <span>·</span>
            <button type="button" onClick={() => setScreen("import")}>
              导入助记词
            </button>
          </div>

          {config.devWallet && (
            <button
              type="button"
              className="tg-welcome-dev"
              disabled={busy}
              onClick={() => void run(() => enterWithDev())}
            >
              开发演示 · {config.devSeed}
            </button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="tg-welcome">
      <div className="tg-welcome-card">
        <button type="button" className="tg-welcome-back" onClick={goWelcome}>
          ← 返回
        </button>

        <h2 className="tg-welcome-card-title">
          {screen === "unlock" && "欢迎回来"}
          {screen === "create" && CREATE_TITLES[createStep]}
          {screen === "import" && "导入账户"}
        </h2>

        {error && (
          <p className="wallet-error">
            {error}
            {error.includes("已保存在本地") ? null : errorHintFor(error)}
          </p>
        )}

        {screen === "unlock" && (
          <section className="wallet-section">
            <ul className="wallet-accts">
              {accounts.map((a) => (
                <li key={a.address}>
                  <button
                    type="button"
                    className={`wallet-acct ${selected === a.address ? "sel" : ""}`}
                    onClick={() => setSelected(a.address)}
                  >
                    <span className="wallet-acct-avatar">{a.name[0]?.toUpperCase() ?? "?"}</span>
                    <span className="wallet-acct-meta">
                      <span className="wallet-acct-name">{a.name}</span>
                      <span className="wallet-acct-addr">{shortAddress(a.address, 10, 6)}</span>
                    </span>
                  </button>
                  <button
                    type="button"
                    className="wallet-del"
                    title="删除"
                    onClick={() =>
                      void run(async () => {
                        if (!confirm(`删除本地账户 ${a.name}？`)) return;
                        await removeAccount(a.address);
                        if (selected === a.address) setSelected(null);
                        await refresh();
                      })
                    }
                  >
                    ✕
                  </button>
                </li>
              ))}
            </ul>
            <label className="wallet-field">
              <span>密码</span>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="current-password"
                placeholder="输入密码解锁"
              />
            </label>
            <button
              type="button"
              className="wallet-primary"
              disabled={busy || !selected || !password}
              onClick={() =>
                void run(async () => {
                  if (!selected) return;
                  await unlockAndEnter(selected, password);
                })
              }
            >
              {busy ? "解锁中…" : "进入聊天"}
            </button>
          </section>
        )}

        {screen === "create" && (
          <CreateWalletFlow
            key={createFlowKey}
            variant="gate"
            onStepChange={setCreateStep}
            onComplete={({ address, name, password: pwd }) =>
              void run(async () => {
                await refresh();
                await unlockAndEnter(address, pwd, name);
              })
            }
          />
        )}

        {screen === "import" && (
          <section className="wallet-section">
            <label className="wallet-field">
              <span>助记词</span>
              <textarea
                value={importMnemonic}
                onChange={(e) => setImportMnemonic(e.target.value)}
                rows={3}
                placeholder="12 或 24 个英文单词，空格分隔"
              />
            </label>
            <label className="wallet-field">
              <span>名称</span>
              <input
                value={importName}
                onChange={(e) => setImportName(e.target.value)}
                placeholder="账户名称"
              />
            </label>
            <label className="wallet-field">
              <span>密码</span>
              <input
                type="password"
                value={importPassword}
                onChange={(e) => setImportPassword(e.target.value)}
                autoComplete="new-password"
                placeholder="至少 8 位，含 2 种字符类型"
              />
            </label>
            <label className="wallet-field">
              <span>确认密码</span>
              <input
                type="password"
                value={importConfirm}
                onChange={(e) => setImportConfirm(e.target.value)}
                autoComplete="new-password"
              />
            </label>
            <button
              type="button"
              className="wallet-primary"
              disabled={
                busy ||
                importMnemonic.trim().length < 10 ||
                importName.length < 1 ||
                importPassword.length < 8
              }
              onClick={() =>
                void run(async () => {
                  const words = importMnemonic.trim().split(/\s+/).length;
                  if (words !== 12 && words !== 24) {
                    setError("助记词须为 12 或 24 个单词");
                    return;
                  }
                  const pwdCheck = validateWalletPassword(importPassword);
                  if (!pwdCheck.valid) {
                    setError(pwdCheck.message);
                    return;
                  }
                  if (importPassword !== importConfirm) {
                    setError("两次密码不一致");
                    return;
                  }
                  await importAndEnter(importMnemonic, importName.trim(), importPassword);
                })
              }
            >
              {busy ? "导入中…" : "导入并进入"}
            </button>
          </section>
        )}
      </div>
    </div>
  );
}
