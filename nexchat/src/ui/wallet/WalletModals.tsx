import { useEffect, useState, type ReactNode } from "react";
import { exportMnemonic, importAccount } from "@/wallet/desktopKeyring";
import { parseNexAmount } from "@/wallet/amount";
import { transferNex } from "@/wallet/transfer";
import { formatBalance } from "@/market/format";
import { validateWalletPassword } from "@/wallet/passwordValidation";
import {
  CreateWalletFlow,
  type CreateWalletStep,
} from "@/ui/wallet/CreateWalletFlow";

type ModalShellProps = {
  title: string;
  onClose: () => void;
  children: ReactNode;
};

function ModalShell({ title, onClose, children }: ModalShellProps) {
  return (
    <div className="wx-wallet-modal-backdrop" onClick={onClose}>
      <div
        className="wx-wallet-modal"
        role="dialog"
        aria-modal
        onClick={(e) => e.stopPropagation()}
      >
        <header className="wx-wallet-modal-head">
          <h3>{title}</h3>
          <button type="button" className="wx-wallet-modal-close" onClick={onClose}>
            ✕
          </button>
        </header>
        <div className="wx-wallet-modal-body">{children}</div>
      </div>
    </div>
  );
}

// EN: Transfer NEX modal.
// CN: NEX 转账弹窗。
export function WalletTransferModal({
  open,
  onClose,
  freeBalance,
  selfAddress,
  initialRecipient = "",
  onSent,
}: {
  open: boolean;
  onClose: () => void;
  freeBalance: bigint;
  selfAddress: string;
  initialRecipient?: string;
  onSent?: () => void;
}) {
  const [recipient, setRecipient] = useState("");
  const [amount, setAmount] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ok, setOk] = useState(false);

  useEffect(() => {
    if (!open) {
      setRecipient("");
      setAmount("");
      setBusy(false);
      setError(null);
      setOk(false);
      return;
    }
    if (initialRecipient.trim()) {
      setRecipient(initialRecipient.trim());
    }
  }, [open, initialRecipient]);

  if (!open) return null;

  const reserve = 100_000_000_000n;
  const maxSend = freeBalance > reserve ? freeBalance - reserve : 0n;

  return (
    <ModalShell title="转账 NEX" onClose={onClose}>
      {ok ? (
        <p className="wx-market-tx-status ok">转账已提交</p>
      ) : (
        <>
          <p className="wx-wallet-modal-hint">
            可用余额 {formatBalance(freeBalance.toString())} NEX
          </p>
          <label className="wx-market-field">
            <span>收款地址</span>
            <input
              className="wx-wallet-input"
              value={recipient}
              onChange={(e) => setRecipient(e.target.value)}
              placeholder="SS58 地址"
            />
          </label>
          <label className="wx-market-field">
            <span>金额 (NEX)</span>
            <div className="wx-wallet-amount-row">
              <input
                className="wx-wallet-input"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                placeholder="0.00"
              />
              <button
                type="button"
                className="wx-wallet-link-btn"
                onClick={() => setAmount(formatBalance(maxSend.toString()))}
              >
                最大
              </button>
            </div>
          </label>
          {error && <p className="wx-market-tx-status error">{error}</p>}
          <button
            type="button"
            className="wx-market-submit buy"
            disabled={busy}
            onClick={() =>
              void (async () => {
                setError(null);
                const planck = parseNexAmount(amount);
                if (!recipient.trim()) {
                  setError("请输入收款地址");
                  return;
                }
                if (recipient.trim() === selfAddress) {
                  setError("不能转账给自己");
                  return;
                }
                if (planck == null || planck <= 0n) {
                  setError("请输入有效金额");
                  return;
                }
                if (planck > freeBalance) {
                  setError("余额不足");
                  return;
                }
                setBusy(true);
                try {
                  await transferNex(recipient.trim(), planck);
                  setOk(true);
                  onSent?.();
                } catch (e) {
                  setError(e instanceof Error ? e.message : String(e));
                } finally {
                  setBusy(false);
                }
              })()
            }
          >
            {busy ? "提交中…" : "确认转账"}
          </button>
        </>
      )}
    </ModalShell>
  );
}

// EN: Receive — show address for copy.
// CN: 收款——展示地址供复制。
export function WalletReceiveModal({
  open,
  onClose,
  address,
}: {
  open: boolean;
  onClose: () => void;
  address: string;
}) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) setCopied(false);
  }, [open]);

  if (!open) return null;

  return (
    <ModalShell title="收款" onClose={onClose}>
      <p className="wx-wallet-modal-hint">向以下地址转入 NEX</p>
      <pre className="wx-wallet-receive-addr">{address}</pre>
      <button
        type="button"
        className="wx-market-submit"
        onClick={() => {
          void navigator.clipboard.writeText(address);
          setCopied(true);
          setTimeout(() => setCopied(false), 2000);
        }}
      >
        {copied ? "已复制" : "复制地址"}
      </button>
    </ModalShell>
  );
}

// EN: Create wallet (name + password + mnemonic backup).
// CN: 创建钱包（名称、密码、助记词备份）。
const CREATE_STEP_TITLES: Record<CreateWalletStep, string> = {
  1: "创建钱包",
  2: "备份助记词",
  3: "验证助记词",
};

export function WalletCreateModal({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (address: string, name: string) => void;
}) {
  const [step, setStep] = useState<CreateWalletStep>(1);
  const [flowKey, setFlowKey] = useState(0);

  useEffect(() => {
    if (!open) {
      setStep(1);
      setFlowKey((k) => k + 1);
    }
  }, [open]);

  if (!open) return null;

  return (
    <ModalShell title={CREATE_STEP_TITLES[step]} onClose={onClose}>
      <CreateWalletFlow
        key={flowKey}
        variant="modal"
        onStepChange={setStep}
        onComplete={({ address, name }) => {
          onCreated(address, name);
          onClose();
        }}
      />
    </ModalShell>
  );
}

// EN: Import wallet from mnemonic.
// CN: 助记词导入钱包。
export function WalletImportModal({
  open,
  onClose,
  onImported,
}: {
  open: boolean;
  onClose: () => void;
  onImported: (address: string, name: string) => void;
}) {
  const [mnemonic, setMnemonic] = useState("");
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setMnemonic("");
      setName("");
      setPassword("");
      setConfirm("");
      setBusy(false);
      setError(null);
    }
  }, [open]);

  if (!open) return null;

  return (
    <ModalShell title="导入钱包" onClose={onClose}>
      <label className="wx-market-field">
        <span>助记词</span>
        <textarea
          className="wx-wallet-textarea"
          rows={3}
          value={mnemonic}
          onChange={(e) => setMnemonic(e.target.value)}
          placeholder="12 或 24 个英文单词"
        />
      </label>
      <label className="wx-market-field">
        <span>钱包名称</span>
        <input
          className="wx-wallet-input"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      <label className="wx-market-field">
        <span>密码</span>
        <input
          className="wx-wallet-input"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
      </label>
      <label className="wx-market-field">
        <span>确认密码</span>
        <input
          className="wx-wallet-input"
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
        />
      </label>
      {error && <p className="wx-market-tx-status error">{error}</p>}
      <button
        type="button"
        className="wx-market-submit buy"
        disabled={busy}
        onClick={() =>
          void (async () => {
            const words = mnemonic.trim().split(/\s+/).length;
            if (words !== 12 && words !== 24) {
              setError("助记词须为 12 或 24 个单词");
              return;
            }
            if (!name.trim()) {
              setError("请输入钱包名称");
              return;
            }
            const pwdCheck = validateWalletPassword(password);
            if (!pwdCheck.valid) {
              setError(pwdCheck.message);
              return;
            }
            if (password !== confirm) {
              setError("两次密码不一致");
              return;
            }
            setError(null);
            setBusy(true);
            try {
              await new Promise((r) => setTimeout(r, 30));
              const result = await importAccount(mnemonic.trim(), name.trim(), password);
              onImported(result.address, name.trim());
              onClose();
            } catch (e) {
              setError(e instanceof Error ? e.message : String(e));
            } finally {
              setBusy(false);
            }
          })()
        }
      >
        {busy ? "导入中…" : "导入"}
      </button>
    </ModalShell>
  );
}

// EN: Export mnemonic (password-gated).
// CN: 导出助记词（需密码）。
export function WalletExportModal({
  open,
  onClose,
  address,
}: {
  open: boolean;
  onClose: () => void;
  address: string;
}) {
  const [password, setPassword] = useState("");
  const [words, setWords] = useState<string[] | null>(null);
  const [notStored, setNotStored] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) {
      setPassword("");
      setWords(null);
      setNotStored(false);
      setBusy(false);
      setError(null);
      setCopied(false);
    }
  }, [open]);

  if (!open) return null;

  return (
    <ModalShell title="导出助记词" onClose={onClose}>
      {notStored ? (
        <p className="wx-wallet-modal-hint">该账户未保存助记词备份（例如由外部导入的旧账户）。</p>
      ) : words ? (
        <>
          <p className="wx-wallet-warn">切勿向任何人透露助记词。</p>
          <pre className="wx-wallet-mnemonic">{words.join(" ")}</pre>
          <button
            type="button"
            className="wx-market-submit"
            onClick={() => {
              void navigator.clipboard.writeText(words.join(" "));
              setCopied(true);
              setTimeout(() => setCopied(false), 2000);
            }}
          >
            {copied ? "已复制" : "复制助记词"}
          </button>
        </>
      ) : (
        <>
          <label className="wx-market-field">
            <span>钱包密码</span>
            <input
              className="wx-wallet-input"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && password) {
                  void (async () => {
                    setBusy(true);
                    setError(null);
                    try {
                      const m = await exportMnemonic(address, password);
                      if (!m) setNotStored(true);
                      else setWords(m.split(" "));
                    } catch {
                      setError("密码错误或解密失败");
                    } finally {
                      setBusy(false);
                    }
                  })();
                }
              }}
            />
          </label>
          {error && <p className="wx-market-tx-status error">{error}</p>}
          <button
            type="button"
            className="wx-market-submit buy"
            disabled={busy || !password}
            onClick={() =>
              void (async () => {
                setBusy(true);
                setError(null);
                try {
                  const m = await exportMnemonic(address, password);
                  if (!m) setNotStored(true);
                  else setWords(m.split(" "));
                } catch {
                  setError("密码错误或解密失败");
                } finally {
                  setBusy(false);
                }
              })()
            }
          >
            {busy ? "验证中…" : "解锁并显示"}
          </button>
        </>
      )}
    </ModalShell>
  );
}

// EN: Scan QR options — camera or album; result opens transfer with prefilled recipient.
// CN: 扫码选项——相机或相册；识别后打开转账并预填收款地址。
export function WalletScanModal({
  open,
  onClose,
  onScanCamera,
  onScanAlbum,
  busy,
  error,
}: {
  open: boolean;
  onClose: () => void;
  onScanCamera: () => void;
  onScanAlbum: () => void;
  busy?: boolean;
  error?: string | null;
}) {
  if (!open) return null;

  return (
    <ModalShell title="扫描二维码" onClose={busy ? () => {} : onClose}>
      <p className="wx-wallet-modal-hint">请选择识别收款二维码的方式</p>
      {error && <p className="wx-market-tx-status error">{error}</p>}
      <button
        type="button"
        className="wx-market-submit buy"
        disabled={busy}
        onClick={onScanCamera}
      >
        {busy ? "识别中…" : "相机扫码"}
      </button>
      <button
        type="button"
        className="wx-market-submit"
        disabled={busy}
        onClick={onScanAlbum}
      >
        从相册选择
      </button>
    </ModalShell>
  );
}
