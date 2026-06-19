// EN: User-facing 1:1 MLS handshake status copy (contacts badge, detail).
// CN: 1:1 MLS 握手状态的用户可见文案（通讯录标记、详情页）。

import type { DirectMlsStatus } from "@/mls/directHandshake";

/// EN: Badge text for contact list — reflects handshake phase, not relay presence.
/// CN: 通讯录标记文案——反映握手阶段，而非 relay 在线与否。
export function directMlsBadgeText(
  status: Pick<DirectMlsStatus, "ready" | "role"> | undefined,
): string {
  if (status?.ready) return "E2EE 就绪";
  if (status?.role === "owner") return "握手中·等对端 KP";
  if (status?.role === "member") return "握手中·等 Welcome";
  return "E2EE 握手中";
}

/// EN: Longer hint for contact detail hero. CN: 联系人详情页较长提示。
export function directMlsDetailHint(
  status: Pick<DirectMlsStatus, "ready" | "role"> | undefined,
): string {
  if (status?.ready) return "🔒 E2EE 就绪";
  if (status?.role === "owner") {
    return "⏳ 握手中：等待对端 KeyPackage（对端需打开 NexChat 并完成好友确认）";
  }
  if (status?.role === "member") {
    return "⏳ 握手中：等待 Welcome（对端在线后会完成加密通道）";
  }
  return "⏳ E2EE 握手中";
}
