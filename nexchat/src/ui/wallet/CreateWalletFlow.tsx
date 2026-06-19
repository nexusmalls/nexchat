import { useEffect, useState } from "react";
import { createAccount } from "@/wallet/desktopKeyring";
import {
  buildMnemonicVerifyQuiz,
  verifyMnemonicSelections,
} from "@/wallet/mnemonicVerify";
import { validateWalletPassword } from "@/wallet/passwordValidation";

export type CreateWalletStep = 1 | 2 | 3;

export type CreateWalletFlowProps = {
  variant: "gate" | "modal";
  onComplete: (result: { address: string; name: string; password: string }) => void;
  onStepChange?: (step: CreateWalletStep) => void;
};

function cx(variant: "gate" | "modal", gate: string, modal: string): string {
  return variant === "gate" ? gate : modal;
}

// EN: 3-step wallet creation — name/password → mnemonic backup → word verification.
// CN: 三步创建钱包——名称密码 → 助记词备份 → 选词验证。
export function CreateWalletFlow({ variant, onComplete, onStepChange }: CreateWalletFlowProps) {
  const [step, setStep] = useState<CreateWalletStep>(1);
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [showPwd, setShowPwd] = useState(false);
  const [mnemonic, setMnemonic] = useState("");
  const [address, setAddress] = useState("");
  const [mnemonicCopied, setMnemonicCopied] = useState(false);
  const [verifyIndices, setVerifyIndices] = useState<number[]>([]);
  const [verifySelections, setVerifySelections] = useState<string[]>(["", "", ""]);
  const [candidateWords, setCandidateWords] = useState<string[][]>([]);
  const [verifyError, setVerifyError] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onStepChange?.(step);
  }, [step, onStepChange]);

  const fieldClass = cx(variant, "wallet-field", "wx-market-field");
  const inputClass = cx(variant, "", "wx-wallet-input");
  const primaryClass = cx(variant, "wallet-primary", "wx-market-submit buy");
  const secondaryClass = cx(variant, "wallet-secondary", "wx-market-submit");
  const warnClass = cx(variant, "wallet-warn", "wx-wallet-warn");

  async function handleStep1() {
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
      await new Promise((r) => setTimeout(r, 50));
      const result = await createAccount(name.trim(), password);
      setMnemonic(result.mnemonic);
      setAddress(result.address);
      setStep(2);
    } catch (e) {
      setError(e instanceof Error ? e.message : "创建失败");
    } finally {
      setBusy(false);
    }
  }

  function handleCopyMnemonic() {
    void navigator.clipboard.writeText(mnemonic);
    setMnemonicCopied(true);
    setTimeout(() => setMnemonicCopied(false), 2000);
    setTimeout(() => {
      void navigator.clipboard.writeText("").catch(() => {});
    }, 30_000);
  }

  function handleGoToStep3() {
    const quiz = buildMnemonicVerifyQuiz(mnemonic);
    setVerifyIndices(quiz.indices);
    setCandidateWords(quiz.candidateWords);
    setVerifySelections(["", "", ""]);
    setVerifyError(false);
    setStep(3);
  }

  function handleVerifyAndFinish() {
    const ok = verifyMnemonicSelections(mnemonic, verifyIndices, verifySelections);
    if (!ok || verifySelections.some((s) => !s)) {
      setVerifyError(true);
      return;
    }
    onComplete({ address, name: name.trim(), password });
  }

  const words = mnemonic ? mnemonic.split(" ") : [];

  if (step === 1) {
    return (
      <section className={cx(variant, "wallet-section", "")}>
        {variant === "modal" && (
          <p className="wx-wallet-modal-hint">设置钱包名称和密码</p>
        )}
        <label className={fieldClass}>
          <span>{variant === "gate" ? "显示名称" : "钱包名称"}</span>
          <input
            className={inputClass || undefined}
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={variant === "gate" ? "你的名字" : "我的钱包"}
            disabled={busy}
          />
        </label>
        <label className={fieldClass}>
          <span>密码</span>
          <div className="wx-wallet-pwd-row">
            <input
              className={inputClass || undefined}
              type={showPwd ? "text" : "password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="new-password"
              placeholder="至少 8 位，含 2 种字符类型"
              disabled={busy}
            />
            <button
              type="button"
              className="wx-wallet-pwd-toggle"
              onClick={() => setShowPwd((v) => !v)}
              tabIndex={-1}
            >
              {showPwd ? "隐藏" : "显示"}
            </button>
          </div>
        </label>
        <label className={fieldClass}>
          <span>确认密码</span>
          <input
            className={inputClass || undefined}
            type="password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            autoComplete="new-password"
            disabled={busy}
          />
        </label>
        {error && (
          <p className={cx(variant, "wallet-error", "wx-market-tx-status error")}>{error}</p>
        )}
        {busy && variant === "modal" && (
          <p className="wx-wallet-modal-hint">正在生成安全密钥，请稍候…</p>
        )}
        <button
          type="button"
          className={primaryClass}
          disabled={busy || name.length < 1 || password.length < 8}
          onClick={() => void handleStep1()}
        >
          {busy ? "创建中…" : "下一步"}
        </button>
      </section>
    );
  }

  if (step === 2) {
    return (
      <section className={cx(variant, "wallet-section", "")}>
        {variant === "modal" && (
          <p className="wx-wallet-modal-hint">请妥善保存助记词</p>
        )}
        <p className={warnClass}>
          请立即将以下助记词抄写到安全的地方。丢失助记词将无法恢复钱包！
        </p>
        <div className="wx-wallet-mnemonic-grid">
          {words.map((word, i) => (
            <div key={i} className="wx-wallet-mnemonic-cell">
              <span className="wx-wallet-mnemonic-num">{i + 1}</span>
              <span className="mono">{word}</span>
            </div>
          ))}
        </div>
        <button type="button" className={secondaryClass} onClick={handleCopyMnemonic}>
          {mnemonicCopied ? "已复制" : "复制助记词"}
        </button>
        <button type="button" className={primaryClass} onClick={handleGoToStep3}>
          我已保存助记词
        </button>
      </section>
    );
  }

  return (
    <section className={cx(variant, "wallet-section", "")}>
      <div className="wx-wallet-verify-addr">
        <span className="wx-market-label">钱包地址</span>
        <p className="mono wx-wallet-receive-addr">{address}</p>
      </div>
      <p className="wx-wallet-verify-title">验证助记词</p>
      <p className="wx-wallet-modal-hint">
        点击选择每个位置对应的正确单词，以验证您已保存
      </p>
      <div className="wx-wallet-verify-rows">
        {verifyIndices.map((idx, i) => (
          <div key={idx} className="wx-wallet-verify-row">
            <span className="wx-market-label">第 {idx + 1} 个单词</span>
            <div className="wx-wallet-verify-options">
              {(candidateWords[i] ?? []).map((word) => {
                const selected = verifySelections[i] === word;
                return (
                  <button
                    key={`${idx}-${word}`}
                    type="button"
                    className={`wx-wallet-verify-opt${selected ? " on" : ""}`}
                    onClick={() => {
                      const next = [...verifySelections];
                      next[i] = selected ? "" : word;
                      setVerifySelections(next);
                      setVerifyError(false);
                    }}
                  >
                    {word}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </div>
      {verifyError && (
        <p className={cx(variant, "wallet-error", "wx-market-tx-status error")}>
          验证失败，请检查输入
        </p>
      )}
      <button
        type="button"
        className={primaryClass}
        disabled={verifySelections.some((s) => !s)}
        onClick={handleVerifyAndFinish}
      >
        完成
      </button>
    </section>
  );
}
