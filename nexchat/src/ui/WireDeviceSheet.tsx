// EN: 1:1 Wire multi-leaf device & security sheet (design §8 / H5 安全 UX). Discloses the privacy cost of
// Wire multi-leaf (the peer can SEE how many devices you have), confirms every listed leaf is E2EI
// account-verified (§3.9 — membership == a verified device), and lets the user remove one of their OWN
// other devices for per-device PCS self-heal (the removed device can no longer read FUTURE messages,
// without rotating the mnemonic). The local device is never offered for removal (would lock the user out).
// CN: 1:1 Wire 多 leaf 设备与安全面板（设计 §8 / H5 安全 UX）。披露 Wire 多 leaf 的隐私代价（对端**能看到**
// 你有几台设备）、确认每个在列 leaf 均经 E2EI 账户验证（§3.9——在列即已验证设备），并允许用户移除自己**其他**
// 设备以做按设备 PCS 自愈（被移除设备无法再读**未来**消息，且无需轮换助记词）。本机设备绝不提供移除（以免把
// 用户自己锁出）。

import { useState } from "react";
import { removeWireDevice } from "@/state/appStore";
import type { WireDevice, WireDeviceRoster } from "@/mls/wireDeviceRoster";

export interface WireDeviceSheetProps {
  open: boolean;
  convId: string;
  roster: WireDeviceRoster;
  peerTitle: string;
  onClose: () => void;
}

export function WireDeviceSheet({ open, convId, roster, peerTitle, onClose }: WireDeviceSheetProps) {
  const [busy, setBusy] = useState<string | null>(null);
  if (!open) return null;

  const onRemove = async (device: WireDevice) => {
    const ok = confirm(
      `移除设备「${device.deviceId}」？\n\n` +
        "该设备将无法再读取本会话的未来消息（按设备 PCS 自愈），历史消息不受影响，且无需更换助记词。",
    );
    if (!ok) return;
    setBusy(device.identity);
    try {
      await removeWireDevice(convId, device.identity);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="dm-overlay tg-modal-overlay" onClick={onClose}>
      <aside className="wx-action-sheet wx-device-sheet" onClick={(e) => e.stopPropagation()}>
        <h3 className="wx-device-title">设备与安全</h3>

        <p className="wx-device-note">
          这是端到端加密（OpenMLS）的多设备 1:1 会话。每台设备各占一个独立加密身份，因此
          <b>对方能看到你大约有几台设备</b>。所有在列设备都已用账户密钥通过验证（E2EI）。
        </p>

        <section className="wx-device-group">
          <div className="wx-device-group-head">我的设备（{roster.self.length}）</div>
          {roster.self.map((d) => (
            <div className="wx-device-row" key={d.identity}>
              <span className="wx-device-id mono">{d.deviceId}</span>
              <span className="wx-device-flags">
                ✓ 已验证
                {d.isThisDevice && <span className="wx-device-self-tag"> · 本机</span>}
              </span>
              {!d.isThisDevice && (
                <button
                  type="button"
                  className="wx-device-remove"
                  disabled={busy !== null}
                  onClick={() => void onRemove(d)}
                >
                  {busy === d.identity ? "移除中…" : "移除"}
                </button>
              )}
            </div>
          ))}
        </section>

        <section className="wx-device-group">
          <div className="wx-device-group-head">
            {peerTitle} 的设备（{roster.peer.length}）
          </div>
          {roster.peer.map((d) => (
            <div className="wx-device-row" key={d.identity}>
              <span className="wx-device-id mono">{d.deviceId}</span>
              <span className="wx-device-flags">✓ 已验证</span>
            </div>
          ))}
          {roster.peer.length === 0 && <div className="wx-device-empty">对方暂无在此会话的设备</div>}
        </section>

        {roster.other.length > 0 && (
          <section className="wx-device-group">
            <div className="wx-device-group-head">其他（{roster.other.length}）</div>
            {roster.other.map((d) => (
              <div className="wx-device-row" key={d.identity}>
                <span className="wx-device-id mono">{d.account.slice(0, 10)}… · {d.deviceId}</span>
              </div>
            ))}
          </section>
        )}

        <p className="wx-device-foot">
          移除某台设备会触发按设备 PCS：被移除的设备失去未来消息的解密能力，无需更换助记词。
        </p>

        <button type="button" className="wx-action-cancel" onClick={onClose}>
          关闭
        </button>
      </aside>
    </div>
  );
}
