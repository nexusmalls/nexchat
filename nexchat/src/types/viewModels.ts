// EN: Sanitized view models handed to the UI. No keys, no ciphertext, no raw
// SCALE DTOs. Mirrors CHAT_FRONTEND_PLAN.md §3.1.3.
// CN: 交给 UI 的脱敏视图模型。无密钥、无密文、无 SCALE 原始 DTO。
// 对应 CHAT_FRONTEND_PLAN.md §3.1.3。

export type ConvKind = "direct" | "group";
export type GroupRole = "owner" | "admin" | "member" | "na";

/// EN: Source presence of a conversation (diagnostic). CN: 会话来源（诊断用）。
export type ConvPresence = "onChainOnly" | "offChainOnly" | "both";

export type MsgStatus =
  | "pending"
  | "sent"
  | "acked"
  | "failed"
  | "recalled";

export type MsgSource = "offChainMls" | "onChainSystem";

/// EN: One row of the MERGED conversation list (NOT the on-chain slice).
/// CN: 已 Merge 的会话列表中的一行（非链上切片）。
export interface ConversationVM {
  /** 统一主键：direct=peer 派生 id；group=`g:{group_id}` */
  convId: string;
  kind: ConvKind;
  /** 私聊=对端昵称；群=群名 */
  title: string;
  /** 头像 CID（私聊可空） */
  avatarCid?: string;
  /** 私聊对端账户（脱敏展示 id） */
  peer?: string;
  groupId?: number;
  /** 已解密的末条摘要（脱敏，可空） */
  lastMessagePreview?: string;
  /** 排序键 = max(链上折算时间, 链下最后消息时间) */
  recency: number;
  /** 真实未读 = 链下 MLS 未读 (+可选 System) */
  unread: number;
  /** 私聊链上 OR 本地置顶偏好 */
  pinned: boolean;
  /** 免打扰：私聊链上 DND 或本地偏好；群=本地偏好。语义=「收不到提醒」，仍能发言 */
  dnd: boolean;
  /** 仅群：被管理员禁言（不能发言）；私聊恒 false。与 dnd 必须分开渲染 */
  adminMuted: boolean;
  archived: boolean;
  /** 仅群：治理冻结 → UI 只读态 */
  frozen: boolean;
  /** 仅群：成员数 */
  memberCount: number;
  myRole: GroupRole;
  /** 来源诊断（调试/测试用） */
  presence: ConvPresence;
  /** EN: local @me mention unread (groups). CN: 本地「@我」未读（群）。 */
  mentionUnread?: number;
}

export type MessageContent =
  | { type: "text"; text: string }
  | {
      type: "media";
      mime: string;
      name?: string;
      size: number;
      thumbReady: boolean;
      bodyReady: boolean;
      durationMs?: number;
      /** EN: IPFS root CID (encrypted blob or manifest). CN: IPFS 根 CID（密文或 manifest）。 */
      rootCid?: string;
      /** EN: Per-file AES key (base64), E2EE inside MLS envelope. CN: 每文件 AES 密钥（base64），在 MLS 信封内 E2EE。 */
      fileKey?: string;
      thumbCid?: string;
      thumbKey?: string;
      /** EN: `root_cid` points at an encrypted manifest. CN: `root_cid` 指向加密 manifest。 */
      chunked?: boolean;
      /** EN: ephemeral blob URL for optimistic send preview (not persisted). CN: 发送中本地预览（不落盘）。 */
      localPreviewUrl?: string;
    }
  | { type: "system"; kind: string }
  | { type: "reaction"; target: string; emoji: string; op?: "add" | "remove" };

/// EN: One message in a timeline (decrypted, sanitized).
/// CN: 时间线中的一条消息（已解密、脱敏）。
export interface MessageVM {
  clientMsgId: string;
  convId: string;
  senderRef: string;
  isOutgoing: boolean;
  sentAt: number;
  content: MessageContent;
  replyTo?: string;
  mentions: string[];
  /** EN: forward source (P3). CN: 转发来源（P3）。 */
  forwardFrom?: { msgId: string; convId: string; preview?: string };
  ephemeralBurnAt?: number;
  /** EN: TTL from P3 envelope (ms); armed on read when `ephemeralBurnOn === "read"`. CN: P3 信封 TTL（毫秒）；read 模式在打开会话时启动。 */
  ephemeralTtlMs?: number;
  ephemeralBurnOn?: "read" | "deliver";
  starred: boolean;
  status: MsgStatus;
  source: MsgSource;
}

/// EN: Account/session summary returned by unlock/register.
/// CN: unlock/register 返回的账户/会话摘要。
export interface AccountVM {
  account: string;
  nickname?: string;
  /** core 分配的 11 位 id */
  chatUserId?: number;
  /** 在册 KeyPackage 数（不足应补发） */
  keyPackagesAvailable: number;
  inboxRegistered: boolean;
  /** 被治理平台级禁言（作为发送方会被拒） */
  platformMuted: boolean;
}
