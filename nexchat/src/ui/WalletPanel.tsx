import { useCallback, useEffect, useRef, useState, type ChangeEvent } from "react";
import { config } from "@/config";
import { useNexBalance } from "@/hooks/useNexBalance";
import { useNexPrice } from "@/hooks/useNexPrice";
import { useShoppingBalance } from "@/hooks/useShoppingBalance";
import { useStaking } from "@/hooks/useStaking";
import { useWallet } from "@/hooks/useWallet";
import { formatBalance, formatUsdt } from "@/market/format";
import { useUiStore } from "@/state/uiStore";
import { shortAddress } from "@/wallet/address";
import {
  parseQrRecipient,
  QrScanError,
  scanQrCodeFromImage,
} from "@/wallet/qrScanner";
import type { DesktopAccount } from "@/wallet/desktopKeyring";
import { QrCameraModal } from "@/ui/wallet/QrCameraModal";
import {
  WalletCreateModal,
  WalletExportModal,
  WalletImportModal,
  WalletReceiveModal,
  WalletScanModal,
  WalletTransferModal,
} from "@/ui/wallet/WalletModals";

// EN: Me tab — wallet management (mirrors nexus-com-dapp /me/wallet).
// CN: 「我」Tab——钱包管理（对齐 nexus-com-dapp /me/wallet）。
export function WalletPanel() {
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const currentEntityId = useUiStore((s) => s.currentEntityId);
  const entityId = currentEntityId ?? config.defaultEntityId;
  const {
    address,
    name,
    source,
    isConnected,
    loadAccounts,
    lock,
    removeAccount,
    preferAccount,
  } = useWallet();

  const [accounts, setAccounts] = useState<DesktopAccount[]>([]);
  const [accountsLoading, setAccountsLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  const [showTransfer, setShowTransfer] = useState(false);
  const [showReceive, setShowReceive] = useState(false);
  const [showScan, setShowScan] = useState(false);
  const [showCameraScan, setShowCameraScan] = useState(false);
  const [scanBusy, setScanBusy] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [initialRecipient, setInitialRecipient] = useState("");
  const qrImageInputRef = useRef<HTMLInputElement>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [showExport, setShowExport] = useState(false);

  const { balance, loading: balLoading, refresh: refreshBalance } = useNexBalance(
    address,
    !config.useMock,
  );
  const {
    balance: shoppingBal,
    loading: shoppingLoading,
    refresh: refreshShopping,
  } = useShoppingBalance(entityId, address);
  const {
    overview: stakingOverview,
    loading: stakingLoading,
    refresh: refreshStaking,
  } = useStaking(address, !config.useMock);
  const { toUsdt } = useNexPrice(!config.useMock);

  const balanceRefreshing = balLoading || shoppingLoading || stakingLoading;
  const refreshAll = useCallback(() => {
    void refreshBalance();
    void refreshShopping();
    void refreshStaking();
  }, [refreshBalance, refreshShopping, refreshStaking]);

  const refreshAccounts = useCallback(async () => {
    setAccountsLoading(true);
    try {
      setAccounts(await loadAccounts());
    } finally {
      setAccountsLoading(false);
    }
  }, [loadAccounts]);

  useEffect(() => {
    if (!config.useMock) void refreshAccounts();
  }, [refreshAccounts]);

  const free = balance?.free ?? 0n;
  const reserved = balance?.reserved ?? 0n;
  const total = free + reserved;
  const shoppingBalance = BigInt(shoppingBal || "0");
  const staked = stakingOverview?.ledger?.active ?? 0n;
  const totalUsdt = total > 0n && toUsdt ? toUsdt(total) : null;
  const isDesktop = source === "desktop-keyring";
  const currentAcct = accounts.find((a) => a.address === address) ?? null;

  const handleCopy = () => {
    if (!address) return;
    void navigator.clipboard.writeText(address);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const afterNewAccount = (acctAddress: string, acctName: string) => {
    void refreshAccounts();
    if (confirm(`钱包已保存。是否切换至「${acctName}」？切换后需重新输入密码解锁。`)) {
      preferAccount(acctAddress, acctName);
    }
  };

  const openTransferWithScan = useCallback((raw: string): boolean => {
    const recipient = parseQrRecipient(raw);
    if (!recipient) {
      setScanError("未识别到有效收款地址");
      return false;
    }
    setScanError(null);
    setInitialRecipient(recipient);
    setShowScan(false);
    setShowCameraScan(false);
    setShowTransfer(true);
    return true;
  }, []);

  const handleQrImageSelected = useCallback(
    (event: ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = "";
      if (!file) return;

      setScanBusy(true);
      setScanError(null);
      void (async () => {
        try {
          const raw = await scanQrCodeFromImage(file);
          openTransferWithScan(raw);
        } catch (e) {
          if (e instanceof QrScanError) {
            if (e.code === "NO_QR_FOUND") {
              setScanError("图片中未找到二维码");
              return;
            }
            if (e.code === "INVALID_IMAGE") {
              setScanError("无法读取图片");
              return;
            }
          }
          setScanError("扫码失败，请重试");
        } finally {
          setScanBusy(false);
        }
      })();
    },
    [openTransferWithScan],
  );

  const handleTransferClose = () => {
    setInitialRecipient("");
    setShowTransfer(false);
  };

  return (
    <main className="tg-main wx-earnings-main wx-wallet-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={() => setSettingsView("list")}>
          ‹ 返回
        </button>
        <span>钱包管理</span>
        <button
          type="button"
          className="wx-market-refresh"
          onClick={() => void refreshAll()}
          disabled={balanceRefreshing}
        >
          {balanceRefreshing ? "…" : "刷新"}
        </button>
      </header>

      <div className="wx-market-scroll">
        {config.useMock && (
          <div className="wx-market-banner">Mock 模式无真实钱包。请设置 VITE_USE_MOCK=false。</div>
        )}

        {!isConnected || !address ? (
          <section className="wx-market-card wx-wallet-empty-card">
            <p className="wx-wallet-empty-icon">👛</p>
            <p className="wx-market-empty">钱包未连接</p>
            <p className="wx-wallet-modal-hint">请从欢迎页创建或导入钱包后解锁。</p>
            <div className="wx-wallet-action-row">
              <button type="button" className="wx-market-submit buy" onClick={() => setShowCreate(true)}>
                创建钱包
              </button>
              <button type="button" className="wx-market-submit" onClick={() => setShowImport(true)}>
                导入钱包
              </button>
            </div>
          </section>
        ) : (
          <>
            <section className="wx-wallet-card">
              <div className="wx-wallet-card-top">
                <span className="wx-wallet-card-name">{name ?? shortAddress(address)}</span>
                <span className="wx-wallet-card-badge">
                  {source === "dev" ? "Dev" : "本地"}
                </span>
              </div>
              <p className="wx-wallet-card-balance">
                {formatBalance(total.toString())}
                <span className="wx-earnings-unit"> NEX</span>
                {totalUsdt != null && (
                  <span className="wx-wallet-card-usdt"> ≈ ${formatUsdt(totalUsdt)} USDT</span>
                )}
              </p>
              <div className="wx-wallet-card-meta">
                <span>购物余额 {formatBalance(shoppingBalance.toString())}</span>
                <span>质押 {formatBalance(staked.toString())}</span>
                <span>可用余额 {formatBalance(free.toString())}</span>
              </div>
              <div className="wx-wallet-card-addr">
                <span className="mono">{address}</span>
                <button type="button" className="wx-wallet-copy-btn" onClick={handleCopy}>
                  {copied ? "✓" : "复制"}
                </button>
              </div>
            </section>

            <section className="wx-wallet-quick">
              <button
                type="button"
                className="wx-wallet-quick-btn"
                disabled={source === "dev"}
                onClick={() => setShowTransfer(true)}
              >
                <span className="wx-wallet-quick-icon">↗</span>
                <span>转账</span>
              </button>
              <button type="button" className="wx-wallet-quick-btn" onClick={() => setShowReceive(true)}>
                <span className="wx-wallet-quick-icon">◎</span>
                <span>收款</span>
              </button>
              <button
                type="button"
                className="wx-wallet-quick-btn"
                disabled={source === "dev"}
                onClick={() => {
                  setScanError(null);
                  setShowScan(true);
                }}
              >
                <span className="wx-wallet-quick-icon scan">⌁</span>
                <span>扫一扫</span>
              </button>
              {isDesktop && currentAcct && (
                <button type="button" className="wx-wallet-quick-btn" onClick={() => setShowExport(true)}>
                  <span className="wx-wallet-quick-icon">🔑</span>
                  <span>助记词</span>
                </button>
              )}
              <button type="button" className="wx-wallet-quick-btn danger" onClick={lock}>
                <span className="wx-wallet-quick-icon">🔒</span>
                <span>锁定</span>
              </button>
            </section>

            {isDesktop && (
              <section className="wx-market-card">
                <h3 className="wx-market-section-title">添加钱包</h3>
                <div className="wx-wallet-action-row">
                  <button type="button" className="wx-market-submit buy" onClick={() => setShowCreate(true)}>
                    + 创建
                  </button>
                  <button type="button" className="wx-market-submit" onClick={() => setShowImport(true)}>
                    ↓ 导入
                  </button>
                </div>
              </section>
            )}

            {isDesktop && (
              <section className="wx-market-card">
                <h3 className="wx-market-section-title">本地钱包</h3>
                {accountsLoading && accounts.length === 0 ? (
                  <p className="wx-market-empty">加载中…</p>
                ) : accounts.length === 0 ? (
                  <p className="wx-market-empty">暂无本地钱包</p>
                ) : (
                  <div className="wx-wallet-acct-list">
                    {accounts.map((acct) => {
                      const isCurrent = acct.address === address;
                      return (
                        <div
                          key={acct.address}
                          className={`wx-wallet-acct-row${isCurrent ? " current" : ""}`}
                        >
                          <div className="wx-wallet-acct-avatar">
                            {acct.name[0]?.toUpperCase() ?? "?"}
                          </div>
                          <div className="wx-wallet-acct-meta">
                            <span className="wx-wallet-acct-name">
                              {acct.name}
                              {isCurrent && <span className="wx-wallet-acct-tag">当前</span>}
                            </span>
                            <span className="wx-wallet-acct-addr">{shortAddress(acct.address)}</span>
                          </div>
                          <div className="wx-wallet-acct-actions">
                            {!isCurrent && (
                              <>
                                <button
                                  type="button"
                                  className="wx-wallet-acct-switch"
                                  title="切换"
                                  onClick={() => {
                                    if (
                                      confirm(
                                        `切换至「${acct.name}」？将锁定当前会话并返回解锁页。`,
                                      )
                                    ) {
                                      preferAccount(acct.address, acct.name);
                                    }
                                  }}
                                >
                                  切换
                                </button>
                                <button
                                  type="button"
                                  className="wx-wallet-acct-del"
                                  title="删除"
                                  onClick={() =>
                                    void (async () => {
                                      if (!confirm(`删除本地钱包「${acct.name}」？`)) return;
                                      await removeAccount(acct.address);
                                      await refreshAccounts();
                                    })()
                                  }
                                >
                                  ✕
                                </button>
                              </>
                            )}
                            {isCurrent && <span className="wx-wallet-acct-check">✓</span>}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </section>
            )}

            {source === "dev" && (
              <section className="wx-market-card">
                <p className="wx-wallet-modal-hint">
                  当前为开发演示账户（{config.devSeed}）。完整钱包管理请使用桌面 keyring 账户。
                </p>
              </section>
            )}
          </>
        )}

        <p className="wx-market-foot">购物余额 · 质押 · 可用余额 · 转账</p>
      </div>

      {address && (
        <>
          <input
            ref={qrImageInputRef}
            type="file"
            accept="image/*"
            className="wx-qr-file-input"
            onChange={handleQrImageSelected}
          />
          <WalletTransferModal
            open={showTransfer}
            onClose={handleTransferClose}
            freeBalance={free}
            selfAddress={address}
            initialRecipient={initialRecipient}
            onSent={() => void refreshAll()}
          />
          <WalletReceiveModal
            open={showReceive}
            onClose={() => setShowReceive(false)}
            address={address}
          />
          <WalletScanModal
            open={showScan}
            onClose={() => {
              if (scanBusy) return;
              setScanError(null);
              setShowScan(false);
            }}
            busy={scanBusy}
            error={scanError}
            onScanCamera={() => {
              setScanError(null);
              setShowScan(false);
              setShowCameraScan(true);
            }}
            onScanAlbum={() => qrImageInputRef.current?.click()}
          />
          <QrCameraModal
            open={showCameraScan}
            onClose={() => setShowCameraScan(false)}
            onScan={(raw) => {
              if (!openTransferWithScan(raw)) {
                setShowCameraScan(false);
                setShowScan(true);
              }
            }}
          />
          {isDesktop && (
            <WalletExportModal
              open={showExport}
              onClose={() => setShowExport(false)}
              address={address}
            />
          )}
        </>
      )}

      <WalletCreateModal
        open={showCreate}
        onClose={() => setShowCreate(false)}
        onCreated={afterNewAccount}
      />
      <WalletImportModal
        open={showImport}
        onClose={() => setShowImport(false)}
        onImported={afterNewAccount}
      />
    </main>
  );
}
