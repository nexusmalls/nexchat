// EN: Group Wire multi-leaf device & security sheet (CHAT_GROUP_WIREIFY_DESIGN §9, G6/G7). Group twin of the
// 1:1 `WireDeviceSheet`, reusing the same `wx-device-*` styles. It discloses the privacy cost of group
// Wire multi-leaf — every member can SEE roughly how many devices you run, because each device is its own
// MLS leaf — and confirms every listed leaf is E2EI account-verified (§6.4 — membership == a verified
// device). It lists MY devices and the group's other-member devices (grouped by people count), and (G7)
// lets the user remove one of their OWN other devices via the live `GroupWireSession.removeDevice` chain-
// ordered commit (per-device PCS self-heal — the removed device loses FUTURE group messages only).
//
// CN: 群 Wire 多 leaf 设备与安全面板（设计 §9，G6/G7）。1:1 `WireDeviceSheet` 的群侧孪生，复用同套 `wx-device-*`
// 样式。它披露群 Wire 多 leaf 的隐私代价——每个成员**能看到你大约有几台设备**（每台设备各占一个 MLS leaf）——
// 并确认每个在列 leaf 均经 E2EI 账户验证（§6.4——在列即已验证设备）。列出我的设备与群内其他成员设备（按人数分组），
// 并（G7）允许用户经实时 `GroupWireSession.removeDevice` 链定序 commit 移除自己**其他**设备（按设备 PCS 自愈——
// 被移除设备仅失去**未来**群消息）。

import { useState } from "react";
import type { WireDevice, WireGroupRoster } from "@/mls/wireDeviceRoster";

export interface WireGroupDeviceSheetProps {
  open: boolean;
  /** EN: UI conv id (= group MLS key `g:<id>`), passed to the remove action. CN: UI 会话 id（= 群 MLS key
   *  `g:<id>`），传给移除动作。 */
  convId: string;
  roster: WireGroupRoster;
  groupTitle: string;
  /** EN: Remove one of MY OWN other devices from this group (live `removeGroupWireDevice`). Absent → the
   *  sheet stays disclosure-only. CN: 把我自己**其他**设备移出本群（实时 `removeGroupWireDevice`）。缺省 →
   *  面板保持仅披露。 */
  onRemove?: (convId: string, deviceIdentity: string) => Promise<boolean>;
  onClose: () => void;
}

export function WireGroupDeviceSheet({
  open,
  convId,
  roster,
  groupTitle,
  onRemove,
  onClose,
}: WireGroupDeviceSheetProps) {
  const [busy, setBusy] = useState<string | null>(null);
  if (!open) return null;

  const removeDevice = async (device: WireDevice) => {
    if (!onRemove) return;
    const ok = confirm(
      `移除设备「${device.deviceId}」？\n\n` +
        "该设备将无法再读取本群的未来消息（按设备 PCS 自愈），历史消息不受影响，且无需更换助记词。",
    );
    if (!ok) return;
    setBusy(device.identity);
    try {
      await onRemove(convId, device.identity);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="dm-overlay tg-modal-overlay" onClick={onClose}>
      <aside className="wx-action-sheet wx-device-sheet" onClick={(e) => e.stopPropagation()}>
        <h3 className="wx-device-title">设备与安全</h3>

        <p className="wx-device-note">
          这是端到端加密（OpenMLS）的群聊。每台设备各占一个独立加密身份，因此
          <b>群成员能看到你大约有几台设备</b>。所有在列设备都已用账户密钥通过验证（E2EI）。
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
              {onRemove && !d.isThisDevice && (
                <button
                  type="button"
                  className="wx-device-remove"
                  disabled={busy !== null}
                  onClick={() => void removeDevice(d)}
                >
                  {busy === d.identity ? "移除中…" : "移除"}
                </button>
              )}
            </div>
          ))}
          {roster.self.length === 0 && <div className="wx-device-empty">本账户暂无在此群的设备</div>}
        </section>

        <section className="wx-device-group">
          <div className="wx-device-group-head">
            {groupTitle} · 其他成员设备（{roster.members.length} 台 / {roster.memberAccounts} 人）
          </div>
          {roster.members.map((d) => (
            <div className="wx-device-row" key={d.identity}>
              <span className="wx-device-id mono">
                {d.account.slice(0, 10)}… · {d.deviceId}
              </span>
              <span className="wx-device-flags">✓ 已验证</span>
            </div>
          ))}
          {roster.members.length === 0 && <div className="wx-device-empty">群内暂无其他成员设备</div>}
        </section>

        <p className="wx-device-foot">
          群内多端可并发收发；移除某台设备会触发按设备 PCS（被移除设备失去未来消息的解密能力，无需更换助记词）。
        </p>

        <button type="button" className="wx-action-cancel" onClick={onClose}>
          关闭
        </button>
      </aside>
    </div>
  );
}
