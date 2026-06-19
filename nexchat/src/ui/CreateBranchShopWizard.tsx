import { useMemo, useState } from "react";
import { chainClient } from "@/chain/chainClient";
import { fetchShopsByEntity } from "@/shop/entityQueries";
import { createShop } from "@/shop/entityShopTx";
import { MIN_INITIAL_FUND_USDT } from "@/shop/format";
import { useMarketTx } from "@/hooks/useMarketTx";
import { useNexPrice } from "@/hooks/useNexPrice";
import { useTranslations } from "@/i18n";
import { config } from "@/config";

interface CreateBranchShopWizardProps {
  open: boolean;
  entityId: number;
  entityName: string;
  onClose: () => void;
  onSuccess: (shopId: number) => void;
}

async function pollCreatedShop(entityId: number, name: string): Promise<number> {
  const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
    typeof fetchShopsByEntity
  >[0];

  for (let i = 0; i < 6; i += 1) {
    const shops = await fetchShopsByEntity(api, entityId);
    const match = shops.find((s) => s.name === name) ?? shops.sort((a, b) => b.id - a.id)[0];
    if (match) return match.id;
    await new Promise((r) => setTimeout(r, 1200));
  }
  throw new Error("链上尚未读到新店铺，请稍后刷新");
}

// EN: Create branch shop under owned entity (`entityShop.createShop`).
// CN: 在已拥有 Entity 下创建分店（`entityShop.createShop`）。
export function CreateBranchShopWizard({
  open,
  entityId,
  entityName,
  onClose,
  onSuccess,
}: CreateBranchShopWizardProps) {
  const t = useTranslations("branchShop");
  const { toNex, loading: rateLoading } = useNexPrice(open && !config.useMock);

  const minFundNex = useMemo(() => {
    const raw = toNex(MIN_INITIAL_FUND_USDT);
    if (!raw) return null;
    const planck = BigInt(raw);
    const whole = planck / 10n ** 12n;
    const frac = planck % 10n ** 12n;
    const fracStr = frac.toString().padStart(12, "0").replace(/0+$/, "");
    return fracStr ? `${whole}.${fracStr}` : `${whole}`;
  }, [toNex]);

  const [name, setName] = useState("");
  const [initialFund, setInitialFund] = useState("");
  const [createdShopId, setCreatedShopId] = useState<number | null>(null);

  const tx = useMarketTx();

  if (!open) return null;

  function resetForm() {
    setName("");
    setInitialFund("");
    setCreatedShopId(null);
    tx.reset();
  }

  function handleClose() {
    if (tx.busy) return;
    resetForm();
    onClose();
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed || tx.busy) return;
    tx.reset();
    await tx.run(async () => {
      await createShop({
        entityId,
        name: trimmed,
        initialFundNex: initialFund.trim(),
      });
      const shopId = await pollCreatedShop(entityId, trimmed);
      setCreatedShopId(shopId);
      onSuccess(shopId);
      return "ok";
    });
  }

  return (
    <div className="wx-earnings-entity-overlay" onClick={handleClose}>
      <aside className="wx-earnings-entity-panel wx-open-shop-panel" onClick={(e) => e.stopPropagation()}>
        <header className="wx-earnings-entity-head">
          <span>{t("title")}</span>
          <button type="button" onClick={handleClose} aria-label={t("close")} disabled={tx.busy}>
            ✕
          </button>
        </header>

        {tx.status === "ok" && createdShopId != null ? (
          <div className="wx-open-shop-success">
            <p className="wx-open-shop-success-title">{t("successTitle")}</p>
            <p className="wx-shop-order-sub">
              {name.trim()} · {t("shopId", { id: createdShopId })}
            </p>
            <div className="wx-open-shop-success-actions">
              <button type="button" className="wx-earnings-entity-switch-btn" onClick={handleClose}>
                {t("done")}
              </button>
            </div>
          </div>
        ) : (
          <>
            <p className="wx-earnings-entity-desc">
              {t("desc", { entity: entityName, id: entityId })}
            </p>
            <p className="wx-open-shop-fee">
              {t("fundNote")}
              {minFundNex && !rateLoading ? ` ${t("minFundHint", { nex: minFundNex })}` : ""}
            </p>
            <form className="wx-open-shop-form" onSubmit={(e) => void handleSubmit(e)}>
              <label className="wx-market-field">
                <span>{t("shopName")}</span>
                <input
                  className="wx-earnings-entity-select"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder={t("shopNamePlaceholder")}
                  maxLength={64}
                  required
                  disabled={tx.busy}
                />
              </label>
              <label className="wx-market-field">
                <span>{t("initialFund")}</span>
                <input
                  className="wx-earnings-entity-select"
                  value={initialFund}
                  onChange={(e) => setInitialFund(e.target.value)}
                  placeholder={minFundNex ?? "0.0"}
                  inputMode="decimal"
                  required
                  disabled={tx.busy}
                />
              </label>
              {tx.error && <p className="wx-market-tx-status error">{tx.error}</p>}
              <button
                type="submit"
                className="wx-market-submit buy wx-open-shop-submit"
                disabled={!name.trim() || !initialFund.trim() || tx.busy}
              >
                {tx.busy ? t("submitting") : t("submit")}
              </button>
            </form>
          </>
        )}
      </aside>
    </div>
  );
}
