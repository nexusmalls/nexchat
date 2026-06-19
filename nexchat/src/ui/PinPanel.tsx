// EN: IPFS Pin status + renew_pin UI (storage-service billing / grace).
// CN: IPFS Pin 状态 + renew_pin 续费 UI（storage-service 计费/宽限期）。

import { useAppStore } from "@/state/appStore";
import type { PinRow } from "@/chain/pinQueries";

export function PinPanel() {
  const { pins, pinsOpen, setPinsOpen, refreshPins, renewPin, pinsLoading } = useAppStore();
  if (!pinsOpen) return null;

  return (
    <div className="pin-overlay" onClick={() => setPinsOpen(false)}>
      <aside className="pin-panel" onClick={(e) => e.stopPropagation()}>
        <header className="pin-head">
          <span>IPFS Pin 管理</span>
          <button onClick={() => void refreshPins()} disabled={pinsLoading}>
            {pinsLoading ? "刷新…" : "刷新"}
          </button>
          <button onClick={() => setPinsOpen(false)}>✕</button>
        </header>
        {pins.length === 0 ? (
          <p className="pin-empty">暂无链上 Pin（发送非 ephemeral 附件且开启 VITE_IPFS_PIN_ENABLED 后会出现在此）</p>
        ) : (
          <ul className="pin-list">
            {pins.map((p) => (
              <PinRowView key={p.cidHash} row={p} onRenew={(n) => void renewPin(p.cidHash, n)} />
            ))}
          </ul>
        )}
      </aside>
    </div>
  );
}

function PinRowView({ row, onRenew }: { row: PinRow; onRenew: (periods: number) => void }) {
  const graceLabel =
    row.grace === "inGrace"
      ? `宽限期至块 #${row.graceExpiresBlock ?? "?"}`
      : row.grace === "expired"
        ? "已过期（待 unpin）"
        : "正常";
  return (
    <li className={`pin-row grace-${row.grace}`}>
      <div className="pin-cid" title={row.cidHash}>
        {row.cid}
      </div>
      <div className="pin-meta">
        {(row.sizeBytes / 1024).toFixed(0)} KB · {row.replicas} 副本 · 下次计费块 #{row.dueBlock}
      </div>
      <div className="pin-grace">{graceLabel}</div>
      <button className="pin-renew" onClick={() => onRenew(1)}>
        续费 1 周期
      </button>
    </li>
  );
}
