// EN: Global recovery banner for a read-only (Track A escrow-restored) device (design §5.4/§7.3).
// A device whose local MLS snapshot was lost restores group state READ-ONLY from the escrow vault and
// holds no signing key, so it cannot send until it regains sending authority via the §5 online handoff
// ("send on this device" → asks an online primary to hand over the signing-key bundle) or an offline
// PIN restore. This surfaces those actions from anywhere — not only inside a group composer — so the
// user is never stranded by a raw `no_signer` error.
// CN: 只读（路线 A 托管恢复）设备的全局恢复横幅（设计 §5.4/§7.3）。本地 MLS 快照丢失的设备从托管 vault
// 只读恢复群状态、不持签名钥，故在经 §5 在线交接（「在此设备发送」→ 请在线主设备移交签名钥 bundle）或离线
// PIN 恢复重获发送权前无法发送。此横幅让这些动作随处可达（不局限于群输入区），用户不会被裸 `no_signer`
// 错误困住。

import { config } from "@/config";
import { useAppStore } from "@/state/appStore";
import { SigningPinRestoreButton } from "@/ui/SigningPinRestoreButton";

export function SendingAuthorityBanner() {
  const groupSendMode = useAppStore((s) => s.groupSendMode);
  const requestGroupSendAuthority = useAppStore((s) => s.requestGroupSendAuthority);
  const setNotice = useAppStore((s) => s.setNotice);

  // EN: Retired on the group side under group Wire (design §10): per-device leaves are never read-only, so
  // there is no send-authority handoff to surface. CN: 群 Wire 下群侧**退役**（设计 §10）：按设备 leaf 永
  // 不只读，无发送权交接可提示。
  if (config.wireGroupMultileafEnabled) return null;
  // EN: Only when the escrow vault is on AND this device resolved to read-only (`secondary`).
  // `restoring`/`primary` need no action. CN: 仅当托管 vault 开启且本设备解析为只读（`secondary`）。
  // `restoring`/`primary` 无需动作。
  if (!config.mlsVaultEnabled || groupSendMode !== "secondary") return null;

  return (
    <div className="tg-offchain-sync warn" role="status">
      <span>此设备为只读（已从云端恢复），暂时无法发送。可在原设备上发送，或在此设备申请发送权。</span>
      <SigningPinRestoreButton
        className="tg-offchain-sync-btn"
        onRestored={() => setNotice("已恢复本设备发送权")}
      />
      <button
        type="button"
        className="tg-offchain-sync-btn"
        onClick={() => void requestGroupSendAuthority()}
        title="向你的主设备申请发送权（在线交接签名密钥）"
      >
        在此设备发送
      </button>
    </div>
  );
}
