// EN: `chatCore.updateChatProfile` — nickname / avatar CID / signature.
// CN: `chatCore.updateChatProfile`——昵称 / 头像 CID / 个性签名。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";

function utf8Hex(str: string): string {
  const bytes = new TextEncoder().encode(str);
  let s = "0x";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

function encOptUtf8(value?: string | null): string | null {
  if (value == null || value.trim() === "") return null;
  return utf8Hex(value.trim());
}

export type UpdateChatProfileParams = {
  nickname?: string | null;
  avatarCid?: string | null;
  signature?: string | null;
};

// EN: Submit profile update; omitted fields are left unchanged on-chain.
// CN: 提交资料更新；未传字段在链上保持不变。
export async function updateChatProfile(params: UpdateChatProfileParams): Promise<string> {
  if (config.useMock) {
    throw new Error("Mock 模式无法更新链上资料");
  }
  return chainClient.signAndSend("chatCore", "updateChatProfile", [
    encOptUtf8(params.nickname),
    encOptUtf8(params.avatarCid),
    encOptUtf8(params.signature),
  ]);
}
