// EN: Amount / TRON validation for NEX market forms (mirrors nexus-com-dapp).
// CN: NEX 市场表单数量与 TRON 校验（与 nexus-com-dapp 对齐）。

export type AssetType = "NEX" | "USDT";

const PRECISION: Record<AssetType, number> = {
  NEX: 12,
  USDT: 6,
};

export interface AmountValidationResult {
  valid: boolean;
  error?: string;
  value?: bigint;
}

export function validateAmount(input: string, assetType: AssetType): AmountValidationResult {
  if (!input || input.trim() === "") {
    return { valid: false, error: "请输入数量" };
  }

  const trimmed = input.trim();
  if (!/^\d+(\.\d+)?$/.test(trimmed)) {
    return { valid: false, error: "数字格式无效" };
  }

  const num = Number(trimmed);
  if (num <= 0) {
    return { valid: false, error: "数量必须大于 0" };
  }

  const maxDecimals = PRECISION[assetType];
  const parts = trimmed.split(".");
  if (parts.length === 2 && parts[1]!.length > maxDecimals) {
    return { valid: false, error: `${assetType} 最多 ${maxDecimals} 位小数` };
  }

  const factor = 10n ** BigInt(maxDecimals);
  const [intPart, decPart = ""] = parts;
  const paddedDec = decPart.padEnd(maxDecimals, "0");
  const value = BigInt(intPart!) * factor + BigInt(paddedDec);

  return { valid: true, value };
}

export function isValidTronAddress(address: string): boolean {
  const trimmed = address.trim();
  return /^T[1-9A-HJ-NP-Za-km-z]{33}$/.test(trimmed);
}

export function estimateTotal(
  priceDisplay: string,
  amountDisplay: string,
  priceDecimals = 6,
  amountDecimals = 12,
  displayDecimals = 2,
): string | null {
  try {
    const priceParts = priceDisplay.split(".");
    const priceFrac = (priceParts[1] || "").padEnd(priceDecimals, "0").slice(0, priceDecimals);
    const priceRaw =
      BigInt(priceParts[0] || "0") * BigInt(10 ** priceDecimals) + BigInt(priceFrac);

    const amtParts = amountDisplay.split(".");
    const amtFrac = (amtParts[1] || "").padEnd(amountDecimals, "0").slice(0, amountDecimals);
    const amtRaw = BigInt(amtParts[0] || "0") * BigInt(10 ** amountDecimals) + BigInt(amtFrac);

    if (priceRaw <= 0n || amtRaw <= 0n) return null;

    const totalRaw = priceRaw * amtRaw;
    const divisor = BigInt(10 ** (priceDecimals + amountDecimals));
    const whole = totalRaw / divisor;
    const frac = totalRaw % divisor;

    if (displayDecimals === 0) return `${whole}`;

    const scale = BigInt(10 ** displayDecimals);
    const fracScaled = (frac * scale) / divisor;
    return `${whole}.${fracScaled.toString().padStart(displayDecimals, "0")}`;
  } catch {
    return null;
  }
}
