// EN: Wallet password strength rules (mirrors nexus-com-dapp password-validation).
// CN: 钱包密码强度规则（对齐 nexus-com-dapp）。

export interface PasswordValidationResult {
  valid: boolean;
  message: string | null;
}

const HAS_UPPER = /[A-Z]/;
const HAS_LOWER = /[a-z]/;
const HAS_DIGIT = /[0-9]/;
const HAS_SPECIAL = /[^A-Za-z0-9]/;

// EN: Min 8 chars; at least 2 of upper / lower / digit / special.
// CN: 至少 8 位；大写、小写、数字、特殊字符中至少含 2 类。
export function validateWalletPassword(password: string): PasswordValidationResult {
  if (password.length < 8) {
    return { valid: false, message: "密码至少 8 位" };
  }
  let categories = 0;
  if (HAS_UPPER.test(password)) categories++;
  if (HAS_LOWER.test(password)) categories++;
  if (HAS_DIGIT.test(password)) categories++;
  if (HAS_SPECIAL.test(password)) categories++;
  if (categories < 2) {
    return {
      valid: false,
      message: "密码需包含大写、小写、数字、特殊字符中至少 2 种",
    };
  }
  return { valid: true, message: null };
}
