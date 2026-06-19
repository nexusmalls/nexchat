// EN: On-chain chat user profile (`chatCore.ChatUserProfiles`).
// CN: 链上聊天用户资料（`chatCore.ChatUserProfiles`）。

export interface ChatUserProfileView {
  nickname: string | null;
  avatarCid: string | null;
  signature: string | null;
}
