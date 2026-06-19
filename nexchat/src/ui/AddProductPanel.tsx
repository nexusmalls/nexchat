import { useState } from "react";
import { chainClient } from "@/chain/chainClient";
import {
  fetchProductsByIds,
  fetchShopProductIds,
} from "@/shop/entityQueries";
import { createProduct, publishProduct } from "@/shop/entityProductTx";
import { parseUsdtInput } from "@/shop/format";
import type { EntityProduct, ProductCategory, ProductVisibility } from "@/shop/types";
import { useEntityShop } from "@/hooks/useEntityShop";
import { useMarketTx } from "@/hooks/useMarketTx";
import { useOwnedEntities } from "@/hooks/useOwnedEntities";
import { useWallet } from "@/hooks/useWallet";
import { useTranslations } from "@/i18n";
import { isShopManager } from "@/shop/seller";
import { useUiStore } from "@/state/uiStore";
import { config } from "@/config";

interface AddProductPanelProps {
  shopId: number;
}

const CATEGORIES: ProductCategory[] = ["Physical", "Digital", "Service"];
const VISIBILITIES: ProductVisibility[] = ["Public", "MembersOnly"];

async function pollCreatedProduct(
  shopId: number,
  nameCid: string,
  beforeIds: number[],
): Promise<EntityProduct> {
  const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
    typeof fetchShopProductIds
  >[0];

  for (let i = 0; i < 6; i += 1) {
    const ids = await fetchShopProductIds(api, shopId);
    const newIds = ids.filter((id) => !beforeIds.includes(id));
    if (newIds.length > 0) {
      const products = await fetchProductsByIds(api, newIds);
      const match =
        products.find((p) => p.nameCid === nameCid) ??
        products.sort((a, b) => b.id - a.id)[0];
      if (match) return match;
    }
    await new Promise((r) => setTimeout(r, 1200));
  }
  throw new Error("链上尚未读到新商品，请稍后刷新店铺");
}

// EN: Seller flow — create draft product then publish on chain.
// CN: 卖家上架——链上创建草稿商品并上架。
export function AddProductPanel({ shopId }: AddProductPanelProps) {
  const t = useTranslations("addProduct");
  const backShop = useUiStore((s) => s.backShop);
  const openProductDetail = useUiStore((s) => s.openProductDetail);
  const { address } = useWallet();
  const { ownedEntityIds } = useOwnedEntities(address, !!address && !config.useMock);
  const { catalog, refresh } = useEntityShop(true, address);

  const shop = catalog?.shopById.get(shopId);
  const canManage = isShopManager(shop, address, ownedEntityIds);

  const [nameCid, setNameCid] = useState("");
  const [imagesCid, setImagesCid] = useState("");
  const [detailCid, setDetailCid] = useState("");
  const [priceUsdt, setPriceUsdt] = useState("");
  const [stock, setStock] = useState("999");
  const [category, setCategory] = useState<ProductCategory>("Physical");
  const [visibility, setVisibility] = useState<ProductVisibility>("Public");
  const [created, setCreated] = useState<EntityProduct | null>(null);

  const tx = useMarketTx(() => void refresh());

  if (!canManage && !config.useMock) {
    return (
      <main className="tg-main wx-shop-main">
        <header className="tg-sub-head wx-market-head">
          <button type="button" className="tg-sub-back wx-nav-back" onClick={backShop}>
            ‹ {t("back")}
          </button>
          <span>{t("title")}</span>
        </header>
        <p className="wx-market-empty">{t("noPermission")}</p>
      </main>
    );
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (tx.busy) return;

    const usdtVal = parseUsdtInput(priceUsdt);
    if (usdtVal == null) {
      tx.reset();
      return;
    }
    const stockNum = Number(stock);
    if (!Number.isInteger(stockNum) || stockNum < 0) return;

    const trimmedName = nameCid.trim();
    const trimmedImages = imagesCid.trim();
    const trimmedDetail = detailCid.trim();
    if (!trimmedName || !trimmedImages || !trimmedDetail) return;

    tx.reset();
    await tx.run(async () => {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchShopProductIds
      >[0];
      const beforeIds = await fetchShopProductIds(api, shopId);

      await createProduct({
        shopId,
        nameCid: trimmedName,
        imagesCid: trimmedImages,
        detailCid: trimmedDetail,
        usdtPrice: usdtVal,
        stock: stockNum,
        category,
        minOrderQuantity: 1,
        maxOrderQuantity: 0,
        visibility,
      });

      const product = await pollCreatedProduct(shopId, trimmedName, beforeIds);
      await publishProduct(product.id);
      setCreated(product);
      return "ok";
    });
  }

  function resetForm() {
    setNameCid("");
    setImagesCid("");
    setDetailCid("");
    setPriceUsdt("");
    setStock("999");
    setCategory("Physical");
    setVisibility("Public");
    setCreated(null);
    tx.reset();
  }

  return (
    <main className="tg-main wx-shop-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={backShop}>
          ‹ {t("back")}
        </button>
        <span>{t("title")}</span>
      </header>

      <div className="wx-market-scroll">
        <p className="wx-earnings-entity-desc">{t("desc", { shop: shop?.name || `#${shopId}` })}</p>
        <p className="wx-open-shop-fee">{t("cidHint")}</p>

        {tx.status === "ok" && created ? (
          <section className="wx-market-card wx-open-shop-success">
            <p className="wx-open-shop-success-title">{t("successTitle")}</p>
            <p className="wx-shop-order-sub">
              {t("productId", { id: created.id })} · ${parseUsdtInput(priceUsdt) != null ? priceUsdt : "—"} USDT
            </p>
            <div className="wx-open-shop-success-actions">
              <button
                type="button"
                className="wx-market-submit buy wx-open-shop-submit"
                onClick={() => openProductDetail(created.id, shopId)}
              >
                {t("viewProduct")}
              </button>
              <button type="button" className="wx-earnings-entity-switch-btn" onClick={resetForm}>
                {t("addAnother")}
              </button>
              <button type="button" className="wx-earnings-entity-switch-btn" onClick={backShop}>
                {t("done")}
              </button>
            </div>
          </section>
        ) : (
          <form className="wx-market-card wx-open-shop-form" onSubmit={(e) => void handleSubmit(e)}>
            <label className="wx-market-field">
              <span>{t("nameCid")}</span>
              <input
                className="wx-earnings-entity-select"
                value={nameCid}
                onChange={(e) => setNameCid(e.target.value)}
                placeholder="Qm…"
                required
                disabled={tx.busy}
              />
            </label>
            <label className="wx-market-field">
              <span>{t("imagesCid")}</span>
              <input
                className="wx-earnings-entity-select"
                value={imagesCid}
                onChange={(e) => setImagesCid(e.target.value)}
                placeholder="Qm…"
                required
                disabled={tx.busy}
              />
            </label>
            <label className="wx-market-field">
              <span>{t("detailCid")}</span>
              <input
                className="wx-earnings-entity-select"
                value={detailCid}
                onChange={(e) => setDetailCid(e.target.value)}
                placeholder="Qm…"
                required
                disabled={tx.busy}
              />
            </label>
            <label className="wx-market-field">
              <span>{t("priceUsdt")}</span>
              <input
                className="wx-earnings-entity-select"
                value={priceUsdt}
                onChange={(e) => setPriceUsdt(e.target.value)}
                placeholder="9.99"
                inputMode="decimal"
                required
                disabled={tx.busy}
              />
            </label>
            <label className="wx-market-field">
              <span>{t("stock")}</span>
              <input
                className="wx-earnings-entity-select"
                value={stock}
                onChange={(e) => setStock(e.target.value)}
                placeholder="0 = 无限"
                inputMode="numeric"
                required
                disabled={tx.busy}
              />
            </label>
            <label className="wx-market-field">
              <span>{t("category")}</span>
              <select
                className="wx-earnings-entity-select"
                value={category}
                onChange={(e) => setCategory(e.target.value as ProductCategory)}
                disabled={tx.busy}
              >
                {CATEGORIES.map((c) => (
                  <option key={c} value={c}>
                    {t(`category_${c}`)}
                  </option>
                ))}
              </select>
            </label>
            <label className="wx-market-field">
              <span>{t("visibility")}</span>
              <select
                className="wx-earnings-entity-select"
                value={visibility}
                onChange={(e) => setVisibility(e.target.value as ProductVisibility)}
                disabled={tx.busy}
              >
                {VISIBILITIES.map((v) => (
                  <option key={v} value={v}>
                    {t(`visibility_${v}`)}
                  </option>
                ))}
              </select>
            </label>
            {tx.error && <p className="wx-market-tx-status error">{tx.error}</p>}
            <button
              type="submit"
              className="wx-market-submit buy wx-open-shop-submit"
              disabled={
                tx.busy ||
                !nameCid.trim() ||
                !imagesCid.trim() ||
                !detailCid.trim() ||
                parseUsdtInput(priceUsdt) == null
              }
            >
              {tx.busy ? t("submitting") : t("submit")}
            </button>
          </form>
        )}
      </div>
    </main>
  );
}
