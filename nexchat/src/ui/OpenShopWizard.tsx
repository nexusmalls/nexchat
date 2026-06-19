import { useState } from "react";
import { chainClient } from "@/chain/chainClient";
import { getSignerAddress } from "@/chain/signer";
import { createEntity } from "@/earnings/entityRegistryTx";
import { fetchOwnedEntities } from "@/earnings/entityRegistryQueries";
import type { RegistryEntity } from "@/earnings/types";
import { useMarketTx } from "@/hooks/useMarketTx";
import { useTranslations } from "@/i18n";

interface OpenShopWizardProps {
  open: boolean;
  onClose: () => void;
  onSuccess: (entity: RegistryEntity) => void;
  onViewShop?: (entity: RegistryEntity) => void;
}

async function pollCreatedEntity(name: string): Promise<RegistryEntity> {
  const who = getSignerAddress();
  if (!who) throw new Error("请先解锁钱包");

  const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
    typeof fetchOwnedEntities
  >[0];

  for (let i = 0; i < 5; i += 1) {
    const owned = await fetchOwnedEntities(api, who);
    const match =
      owned.find((ent) => ent.name === name) ?? owned.sort((a, b) => b.id - a.id)[0];
    if (match) return match;
    await new Promise((r) => setTimeout(r, 1200));
  }
  throw new Error("链上尚未读到新实体，请稍后刷新");
}

// EN: Mobile register-shop wizard — `createEntity` (auto primary shop on chain).
// CN: 手机端注册开店向导——`createEntity`（链上自动创建主店）。
export function OpenShopWizard({ open, onClose, onSuccess, onViewShop }: OpenShopWizardProps) {
  const t = useTranslations("openShop");
  const [name, setName] = useState("");
  const [logoCid, setLogoCid] = useState("");
  const [referrer, setReferrer] = useState("");
  const [created, setCreated] = useState<RegistryEntity | null>(null);

  const tx = useMarketTx();

  if (!open) return null;

  function resetForm() {
    setName("");
    setLogoCid("");
    setReferrer("");
    setCreated(null);
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
      await createEntity({
        name: trimmed,
        logoCid: logoCid.trim() || null,
        referrer: referrer.trim() || null,
      });
      const entity = await pollCreatedEntity(trimmed);
      setCreated(entity);
      onSuccess(entity);
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

        {tx.status === "ok" && created ? (
          <div className="wx-open-shop-success">
            <p className="wx-open-shop-success-title">{t("successTitle")}</p>
            <p className="wx-shop-order-sub">
              {created.name} · Entity #{created.id}
              {created.primaryShopId > 0 ? ` · ${t("shopId", { id: created.primaryShopId })}` : ""}
            </p>
            <p className="wx-earnings-entity-desc">{t("successHint")}</p>
            <div className="wx-open-shop-success-actions">
              {created.primaryShopId > 0 && onViewShop && (
                <button
                  type="button"
                  className="wx-market-submit buy wx-open-shop-submit"
                  onClick={() => onViewShop(created)}
                >
                  {t("viewShop")}
                </button>
              )}
              <button type="button" className="wx-earnings-entity-switch-btn" onClick={handleClose}>
                {t("done")}
              </button>
            </div>
          </div>
        ) : (
          <>
            <p className="wx-earnings-entity-desc">{t("desc")}</p>
            <p className="wx-open-shop-fee">{t("feeNote")}</p>
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
                <span>
                  {t("logoCid")} <span className="wx-open-shop-optional">{t("optional")}</span>
                </span>
                <input
                  className="wx-earnings-entity-select"
                  value={logoCid}
                  onChange={(e) => setLogoCid(e.target.value)}
                  placeholder="Qm…"
                  disabled={tx.busy}
                />
              </label>
              <label className="wx-market-field">
                <span>
                  {t("referrer")} <span className="wx-open-shop-optional">{t("optional")}</span>
                </span>
                <input
                  className="wx-earnings-entity-select"
                  value={referrer}
                  onChange={(e) => setReferrer(e.target.value)}
                  placeholder="5Grw…"
                  disabled={tx.busy}
                />
              </label>
              {tx.error && <p className="wx-market-tx-status error">{tx.error}</p>}
              <button
                type="submit"
                className="wx-market-submit buy wx-open-shop-submit"
                disabled={!name.trim() || tx.busy}
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
