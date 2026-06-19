import { useRef, useState } from "react";
import { registerEntityMember } from "@/earnings/entityMemberTx";
import type { RegistryEntity } from "@/earnings/types";
import { useMarketTx } from "@/hooks/useMarketTx";

interface EarningsEntityPickerProps {
  open: boolean;
  onClose: () => void;
  entities: RegistryEntity[];
  entitiesLoading: boolean;
  memberIds: number[];
  selectedEntityId: number | null;
  onSelect: (entityId: number) => void;
  onJoined: () => void;
}

// EN: Modal to select or join an entity (mirrors nexus-com-dapp settings entity dialog).
// CN: 选择或加入 Entity 的弹层（对齐 nexus-com-dapp 设置页实体对话框）。
export function EarningsEntityPicker({
  open,
  onClose,
  entities,
  entitiesLoading,
  memberIds,
  selectedEntityId,
  onSelect,
  onJoined,
}: EarningsEntityPickerProps) {
  const [joiningId, setJoiningId] = useState<number | null>(null);
  const pendingJoinId = useRef<number | null>(null);

  const joinTx = useMarketTx(() => {
    const id = pendingJoinId.current;
    pendingJoinId.current = null;
    setJoiningId(null);
    if (id != null) {
      onSelect(id);
      onClose();
    }
    onJoined();
  });

  if (!open) return null;

  const memberSet = new Set(memberIds);

  async function handlePick(entity: RegistryEntity) {
    if (joinTx.busy) return;
    if (entity.id === selectedEntityId) {
      onClose();
      return;
    }
    if (memberSet.has(entity.id)) {
      onSelect(entity.id);
      onClose();
      return;
    }
    if (!entity.primaryShopId) return;
    pendingJoinId.current = entity.id;
    setJoiningId(entity.id);
    joinTx.reset();
    await joinTx.run(() => registerEntityMember(entity.primaryShopId, null));
  }

  return (
    <div className="wx-earnings-entity-overlay" onClick={onClose}>
      <aside className="wx-earnings-entity-panel" onClick={(e) => e.stopPropagation()}>
        <header className="wx-earnings-entity-head">
          <span>选择实体</span>
          <button type="button" onClick={onClose} aria-label="关闭">
            ✕
          </button>
        </header>
        <p className="wx-earnings-entity-desc">选择要加入的实体</p>

        {entitiesLoading ? (
          <p className="wx-market-empty">加载实体列表…</p>
        ) : entities.length === 0 ? (
          <p className="wx-market-empty">链上暂无可用实体</p>
        ) : (
          <ul className="wx-earnings-entity-list">
            {entities.map((entity) => {
              const isMember = memberSet.has(entity.id);
              const isCurrent = entity.id === selectedEntityId;
              const isJoining = joiningId === entity.id && joinTx.busy;
              return (
                <li key={entity.id}>
                  <button
                    type="button"
                    className={`wx-earnings-entity-row${isCurrent ? " current" : ""}`}
                    disabled={joinTx.busy}
                    onClick={() => void handlePick(entity)}
                  >
                    <div className="wx-earnings-entity-row-main">
                      <span className="wx-earnings-entity-row-name">{entity.name}</span>
                      <span className="wx-earnings-entity-row-meta">
                        ID {entity.id}
                        {entity.verified ? " · 已认证" : ""}
                        {isMember ? " · 已加入" : " · 点击加入"}
                      </span>
                    </div>
                    {isJoining ? (
                      <span className="wx-earnings-entity-row-action">加入中…</span>
                    ) : isCurrent ? (
                      <span className="wx-earnings-entity-row-action current">当前</span>
                    ) : null}
                  </button>
                </li>
              );
            })}
          </ul>
        )}

        {joinTx.status === "error" && (
          <p className="wx-market-tx-status error">{joinTx.error ?? "加入失败"}</p>
        )}
      </aside>
    </div>
  );
}
