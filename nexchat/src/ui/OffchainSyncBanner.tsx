// EN: Banner for cloud backup restore status (contacts / chats sync), plus the §6.5
// post-disaster orchestration notice — shown even after a fully successful restore
// (the success case is exactly when the user must be told to close the replay window).
// ① relay write-back and ② inbox re-registration run automatically (coordinator +
// delivery bootstrap); the banner makes ③ epoch bump actionable and carries the ④
// MLS re-entry guidance (session keys are NOT restorable by design).
// CN: 云端备份恢复状态提示条（通讯录 / 聊天记录同步），叠加 §6.5 灾后编排提示——
// 恢复完全成功时也要展示（成功恢复恰是必须告知用户关闭重放窗口的场景）。① relay
// 写回与 ② inbox 重注册已自动执行（coordinator + delivery bootstrap）；提示条让
// ③ epoch bump 可一键执行，并附 ④ MLS 重入群引导（会话密钥按设计不可恢复）。

import { useState } from "react";
import { config } from "@/config";
import { useAppStore } from "@/state/appStore";

export function OffchainSyncBanner() {
  const status = useAppStore((s) => s.offchainSync);
  const retry = useAppStore((s) => s.retryOffchainSync);
  const dismissEpochBump = useAppStore((s) => s.dismissEpochBump);
  const bumpInboxEpoch = useAppStore((s) => s.bumpInboxEpoch);
  const [bumping, setBumping] = useState(false);
  const [bumpFailed, setBumpFailed] = useState(false);

  if (!status) return null;

  // EN: §6.5 — chain anchor won at least one field → relay lost/stale data → post-
  // disaster notice. Shown once the restore has SETTLED (ok / partial / idle) — it
  // survives phase "ok" (the old banner hid exactly this case) but must NOT displace
  // the in-progress or error+retry banner while recovery is still unresolved.
  // CN: §6.5——链锚至少赢得一个字段→relay 丢失/陈旧→灾后提示。在恢复**落定**
  // （ok / partial / idle）后展示——"ok" 阶段仍显示（旧实现恰好把这种情况藏掉），
  // 但恢复未落定时**不得**顶掉进行中或错误+重试提示条。
  const settled =
    status.phase === "ok" || status.phase === "partial" || status.phase === "idle";
  if (settled && "needsEpochBump" in status && status.needsEpochBump) {
    const onBump = async () => {
      setBumping(true);
      setBumpFailed(false);
      const ok = await bumpInboxEpoch();
      setBumping(false);
      if (!ok) setBumpFailed(true);
    };
    return (
      <div className="tg-offchain-sync warn" role="alert">
        <span>
          已从链上加密锚恢复备份。
          {config.deliveryTokensEnabled
            ? "建议立即重置投递信箱纪元（epoch），作废旧投递令牌、关闭重放窗口；"
            : ""}
          群聊需重新入群、1:1 加密会话需重新握手（会话密钥按设计不在恢复范围）
          {bumpFailed ? "。纪元重置失败，请检查 relay 连接后重试" : ""}
        </span>
        {config.deliveryTokensEnabled && (
          <button
            type="button"
            className="tg-offchain-sync-btn"
            disabled={bumping}
            onClick={() => void onBump()}
          >
            {bumping ? "重置中…" : "重置信箱纪元"}
          </button>
        )}
        <button
          type="button"
          className="tg-offchain-sync-btn"
          onClick={() => dismissEpochBump()}
        >
          我知道了
        </button>
      </div>
    );
  }

  if (status.phase === "idle" || status.phase === "ok") {
    if (status.anchorNote) {
      return (
        <div className="tg-offchain-sync warn" role="status">
          <span>{status.anchorNote}</span>
        </div>
      );
    }
    return null;
  }

  const isError = status.phase === "error" || status.phase === "no_backup";
  const label =
    status.phase === "restoring"
      ? "正在从云端恢复通讯录和聊天记录…"
      : status.phase === "pushing"
        ? "正在同步到云端…"
        : status.message ?? "云端同步异常";

  return (
    <div className={`tg-offchain-sync${isError ? " error" : ""}`} role="status">
      <span>{label}</span>
      {isError && (
        <button type="button" className="tg-offchain-sync-btn" onClick={() => void retry()}>
          重试
        </button>
      )}
    </div>
  );
}
