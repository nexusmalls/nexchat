import { useEffect, useMemo, useRef, useState } from "react";
import { chainClient } from "@/chain/chainClient";
import { config } from "@/config";
import { fetchRegistryEntityById } from "@/earnings/entityRegistryQueries";
import type { RegistryEntity } from "@/earnings/types";
import { useAllEntities, useMyMemberships } from "@/hooks/useEntities";
import { useOwnedEntities } from "@/hooks/useOwnedEntities";
import { useWallet } from "@/hooks/useWallet";
import { useTranslations } from "@/i18n";
import { useUiStore } from "@/state/uiStore";
import { EarningsEntityPicker } from "@/ui/EarningsEntityPicker";
import { OpenShopWizard } from "@/ui/OpenShopWizard";
import { CreateBranchShopWizard } from "@/ui/CreateBranchShopWizard";

// EN: Me tab — join/switch entity + register shop (createEntity → primary shop).
// CN: 「我」Tab——加入/切换实体 + 注册开店（createEntity → 主店）。
export function EntityPanel() {
  const t = useTranslations("entityPanel");
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const registerShopOpen = useUiStore((s) => s.registerShopOpen);
  const closeRegisterShop = useUiStore((s) => s.closeRegisterShop);
  const openRegisterShop = useUiStore((s) => s.openRegisterShop);
  const openShopDetail = useUiStore((s) => s.openShopDetail);
  const currentEntityId = useUiStore((s) => s.currentEntityId);
  const currentEntityName = useUiStore((s) => s.currentEntityName);
  const setCurrentEntity = useUiStore((s) => s.setCurrentEntity);
  const { address } = useWallet();
  const {
    entities: registryEntities,
    loading: registryLoading,
    refresh: refreshRegistry,
  } = useAllEntities(!!address);
  const {
    entities: ownedEntities,
    loading: ownedLoading,
    refresh: refreshOwned,
  } = useOwnedEntities(address, !!address);
  const registryIds = useMemo(() => registryEntities.map((e) => e.id), [registryEntities]);
  const { memberIds, refresh: refreshMemberships } = useMyMemberships(
    registryIds,
    address,
    !!address,
  );

  const [pickerOpen, setPickerOpen] = useState(false);
  const [branchShopEntity, setBranchShopEntity] = useState<RegistryEntity | null>(null);
  const [idInput, setIdInput] = useState(() =>
    currentEntityId != null ? String(currentEntityId) : "",
  );
  const [idError, setIdError] = useState<string | null>(null);
  const [idApplying, setIdApplying] = useState(false);
  const userClearedEntityRef = useRef(false);

  useEffect(() => {
    if (currentEntityId != null) setIdInput(String(currentEntityId));
  }, [currentEntityId]);

  const selectedRegistry = registryEntities.find((e) => e.id === currentEntityId);
  const displayName =
    currentEntityName ?? selectedRegistry?.name ?? (currentEntityId != null ? `Entity #${currentEntityId}` : null);

  const joinedEntities = useMemo(() => {
    return memberIds
      .map((id) => registryEntities.find((e) => e.id === id))
      .filter((e): e is NonNullable<typeof e> => e != null);
  }, [memberIds, registryEntities]);

  useEffect(() => {
    if (
      currentEntityId != null ||
      config.useMock ||
      !address ||
      userClearedEntityRef.current
    ) {
      return;
    }

    let cancelled = false;
    void (async () => {
      if (config.defaultEntityId != null) {
        try {
          const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
            typeof fetchRegistryEntityById
          >[0];
          const reg = await fetchRegistryEntityById(api, config.defaultEntityId);
          if (!cancelled && reg) {
            setCurrentEntity(reg.id, reg.name);
            return;
          }
        } catch {
          /* fall through */
        }
      }

      if (cancelled) return;
      if (ownedEntities.length > 0) {
        setCurrentEntity(ownedEntities[0]!.id, ownedEntities[0]!.name);
        return;
      }
      if (memberIds.length > 0) {
        const reg = registryEntities.find((e) => e.id === memberIds[0]);
        setCurrentEntity(memberIds[0]!, reg?.name ?? null);
        return;
      }
      if (registryEntities.length === 1) {
        setCurrentEntity(registryEntities[0]!.id, registryEntities[0]!.name);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [
    currentEntityId,
    address,
    memberIds,
    registryEntities,
    ownedEntities,
    setCurrentEntity,
  ]);

  function handleSelect(entityId: number) {
    userClearedEntityRef.current = false;
    const reg =
      registryEntities.find((e) => e.id === entityId) ??
      ownedEntities.find((e) => e.id === entityId);
    setCurrentEntity(entityId, reg?.name ?? null);
    setIdError(null);
  }

  function exitCurrentEntity() {
    userClearedEntityRef.current = true;
    setCurrentEntity(null, null);
    setIdInput("");
    setIdError(null);
    setPickerOpen(true);
  }

  async function applyEntityById() {
    const trimmed = idInput.trim();
    if (!trimmed) {
      setIdError(t("idRequired"));
      return;
    }
    const entityId = Number(trimmed);
    if (!Number.isFinite(entityId) || entityId <= 0 || !Number.isInteger(entityId)) {
      setIdError(t("idInvalid"));
      return;
    }
    if (config.useMock) {
      setIdError(t("mockBlocked"));
      return;
    }

    setIdApplying(true);
    setIdError(null);
    try {
      const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
        typeof fetchRegistryEntityById
      >[0];
      const reg = await fetchRegistryEntityById(api, entityId);
      if (!reg) {
        setIdError(t("idNotFound", { id: entityId }));
        return;
      }
      setCurrentEntity(reg.id, reg.name);
      setIdInput(String(reg.id));
      userClearedEntityRef.current = false;
    } catch (e) {
      setIdError(e instanceof Error ? e.message : String(e));
    } finally {
      setIdApplying(false);
    }
  }

  function handleJoined() {
    void refreshMemberships();
    void refreshRegistry();
  }

  function handleRegisterSuccess(entity: RegistryEntity) {
    userClearedEntityRef.current = false;
    setCurrentEntity(entity.id, entity.name);
    void refreshOwned();
    void refreshRegistry();
  }

  function handleViewShop(entity: RegistryEntity) {
    closeRegisterShop();
    if (entity.primaryShopId > 0) {
      openShopDetail(entity.primaryShopId);
    }
  }

  return (
    <main className="tg-main wx-earnings-main">
      <OpenShopWizard
        open={registerShopOpen}
        onClose={closeRegisterShop}
        onSuccess={handleRegisterSuccess}
        onViewShop={handleViewShop}
      />
      {branchShopEntity && (
        <CreateBranchShopWizard
          open
          entityId={branchShopEntity.id}
          entityName={branchShopEntity.name}
          onClose={() => setBranchShopEntity(null)}
          onSuccess={(shopId) => {
            void refreshOwned();
            openShopDetail(shopId);
            setBranchShopEntity(null);
          }}
        />
      )}
      <EarningsEntityPicker
        open={pickerOpen}
        onClose={() => setPickerOpen(false)}
        entities={registryEntities}
        entitiesLoading={registryLoading}
        memberIds={memberIds}
        selectedEntityId={currentEntityId}
        onSelect={handleSelect}
        onJoined={handleJoined}
      />

      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={() => setSettingsView("list")}>
          ‹ {t("back")}
        </button>
        <span>{t("title")}</span>
      </header>

      <div className="wx-market-scroll">
        {config.useMock && (
          <div className="wx-market-banner">
            {t("mockBanner")} <code>VITE_USE_MOCK=false</code>
          </div>
        )}
        {!address && <p className="wx-market-empty">{t("unlockWallet")}</p>}

        {address && (
          <>
            <section className="wx-market-card wx-open-shop-cta-card">
              <p className="wx-earnings-entity-desc-inline">{t("registerDesc")}</p>
              <button
                type="button"
                className="wx-market-submit buy wx-open-shop-submit"
                onClick={() => openRegisterShop()}
                disabled={config.useMock}
              >
                {t("registerCta")}
              </button>
            </section>

            {ownedEntities.length > 0 && (
              <section className="wx-market-card wx-earnings-entity-card">
                <p className="wx-market-label">{t("ownedTitle")}</p>
                <ul className="wx-open-shop-owned-list">
                  {ownedEntities.map((ent) => (
                    <li key={ent.id} className="wx-open-shop-owned-row">
                      <div>
                        <p className="wx-earnings-entity-current-name">{ent.name}</p>
                        <p className="wx-shop-order-sub">
                          Entity #{ent.id}
                          {ent.primaryShopId > 0 ? ` · ${t("shopId", { id: ent.primaryShopId })}` : ""}
                        </p>
                      </div>
                      <div className="wx-open-shop-owned-actions">
                        {ent.primaryShopId > 0 && (
                          <button
                            type="button"
                            className="wx-earnings-entity-switch-btn"
                            onClick={() => openShopDetail(ent.primaryShopId)}
                          >
                            {t("viewShop")}
                          </button>
                        )}
                        <button
                          type="button"
                          className="wx-earnings-entity-link-btn"
                          onClick={() => setBranchShopEntity(ent)}
                          disabled={config.useMock}
                        >
                          {t("branchShop")}
                        </button>
                        <button
                          type="button"
                          className="wx-earnings-entity-link-btn"
                          onClick={() => handleSelect(ent.id)}
                        >
                          {currentEntityId === ent.id ? t("current") : t("useEntity")}
                        </button>
                      </div>
                    </li>
                  ))}
                </ul>
                {ownedLoading && <p className="wx-shop-order-sub">{t("loadingOwned")}</p>}
              </section>
            )}

            <section className="wx-market-card wx-earnings-entity-card">
              <p className="wx-earnings-entity-desc-inline">{t("joinDesc")}</p>

              {config.defaultEntityId != null && (
                <p className="wx-shop-order-sub">
                  {t("defaultEntity", { id: config.defaultEntityId })}
                  {currentEntityId == null ? t("defaultEntityHint") : ""}
                </p>
              )}

              {registryLoading && registryEntities.length === 0 && currentEntityId == null ? (
                <p className="wx-market-empty">{t("loadingEntities")}</p>
              ) : displayName && currentEntityId != null ? (
                <div className="wx-earnings-entity-current">
                  <div className="wx-earnings-entity-current-main">
                    <p className="wx-market-label">{t("currentEntity")}</p>
                    <p className="wx-earnings-entity-current-name">{displayName}</p>
                    <p className="wx-shop-order-sub">ID {currentEntityId}</p>
                  </div>
                  <div className="wx-earnings-entity-current-actions">
                    <button
                      type="button"
                      className="wx-earnings-entity-switch-btn"
                      onClick={() => setPickerOpen(true)}
                    >
                      {t("pickFromList")}
                    </button>
                    <button
                      type="button"
                      className="wx-earnings-entity-exit-btn"
                      onClick={exitCurrentEntity}
                    >
                      {t("exitEntity")}
                    </button>
                  </div>
                </div>
              ) : (
                <div className="wx-earnings-entity-empty">
                  <p className="wx-market-empty">{t("noEntitySelected")}</p>
                  <button type="button" className="wx-market-submit buy" onClick={() => setPickerOpen(true)}>
                    {t("pickFromList")}
                  </button>
                </div>
              )}

              <label className="wx-market-field wx-entity-id-field">
                <span>{t("setById")}</span>
                <div className="wx-entity-id-row">
                  <input
                    className="wx-earnings-entity-select"
                    type="number"
                    min={1}
                    step={1}
                    inputMode="numeric"
                    placeholder={t("idPlaceholder")}
                    value={idInput}
                    onChange={(e) => {
                      setIdInput(e.target.value);
                      setIdError(null);
                    }}
                  />
                  <button
                    type="button"
                    className="wx-earnings-entity-switch-btn"
                    disabled={idApplying}
                    onClick={() => void applyEntityById()}
                  >
                    {idApplying ? t("validating") : t("apply")}
                  </button>
                </div>
              </label>
              {idError && <p className="wx-market-tx-status error">{idError}</p>}

              {joinedEntities.length > 1 && (
                <label className="wx-market-field">
                  <span>{t("joinedEntities")}</span>
                  <select
                    className="wx-earnings-entity-select"
                    value={currentEntityId ?? ""}
                    onChange={(e) => handleSelect(Number(e.target.value))}
                  >
                    {joinedEntities.map((e) => (
                      <option key={e.id} value={e.id}>
                        {e.name} (#{e.id})
                      </option>
                    ))}
                  </select>
                </label>
              )}

              <p className="wx-market-foot" style={{ marginTop: 12 }}>
                {t("footNote")}
              </p>
            </section>
          </>
        )}
      </div>
    </main>
  );
}
