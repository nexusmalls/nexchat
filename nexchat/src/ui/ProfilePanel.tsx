import { useRef, useState } from "react";
import { useAppStore } from "@/state/appStore";
import { useUiStore } from "@/state/uiStore";
import { useChainKeyPackageCount } from "@/hooks/useChainKeyPackageCount";
import { useChatProfile } from "@/hooks/useChatProfile";
import { useWallet } from "@/hooks/useWallet";
import { updateChatProfile } from "@/chat/profileTx";
import { config } from "@/config";
import { ipfsClient } from "@/ipfs/ipfsClient";
import { shortAddress } from "@/wallet/address";
import { ProfileAvatar } from "@/ui/ProfileAvatar";
import { copyToClipboard } from "@/util/copyToClipboard";

const NICK_KEY = "nexchat-profile-nickname";

function readLocalNick(fallback: string): string {
  if (typeof localStorage === "undefined") return fallback;
  return localStorage.getItem(NICK_KEY) ?? fallback;
}

// EN: Profile detail — avatar upload (IPFS + chatCore.updateChatProfile), nickname, MLS summary.
// CN: 个人资料——头像上传（IPFS + chatCore.updateChatProfile）、昵称、MLS 摘要。
export function ProfilePanel() {
  const { account, mls, directMls, drPeers, mlsGroupId } = useAppStore();
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const { name, address, source } = useWallet();
  const { profile, loading: profileLoading, refresh: refreshProfile } = useChatProfile(
    address,
    !config.useMock,
  );

  const fileRef = useRef<HTMLInputElement>(null);
  const baseName = profile?.nickname ?? name ?? account?.nickname ?? "用户";
  const [nick, setNick] = useState(() => readLocalNick(baseName));
  const [editing, setEditing] = useState(false);
  const [avatarBusy, setAvatarBusy] = useState(false);
  const [nickBusy, setNickBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const displayAddr = address ?? account?.account ?? "—";
  const {
    count: chainKeyPackageCount,
    loading: chainKeyPackageLoading,
  } = useChainKeyPackageCount(displayAddr !== "—" ? displayAddr : null, !config.useMock);
  const directReady =
    Object.values(directMls).filter((s) => s.ready).length +
    Object.values(drPeers).filter(Boolean).length;
  const displayName = profile?.nickname ?? nick;

  async function saveNick() {
    const v = nick.trim() || baseName;
    setNick(v);
    localStorage.setItem(NICK_KEY, v);
    setEditing(false);
    if (config.useMock || !address) return;
    setNickBusy(true);
    setError(null);
    try {
      await updateChatProfile({ nickname: v });
      await refreshProfile();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setNickBusy(false);
    }
  }

  async function onAvatarSelected(file: File) {
    if (!config.ipfsEnabled) {
      setError("IPFS 未启用，无法上传头像");
      return;
    }
    if (config.useMock || !address) {
      setError("请关闭 Mock 并解锁钱包后再上传");
      return;
    }
    if (!file.type.startsWith("image/")) {
      setError("请选择图片文件");
      return;
    }
    setAvatarBusy(true);
    setError(null);
    try {
      const plain = new Uint8Array(await file.arrayBuffer());
      const cid = await ipfsClient.add(plain, file.name);
      await updateChatProfile({ avatarCid: cid });
      await refreshProfile();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setAvatarBusy(false);
    }
  }

  const canUploadAvatar = !config.useMock && !!address && config.ipfsEnabled;

  return (
    <main className="tg-main tg-settings-main">
      <header className="tg-sub-head">
        <button type="button" className="tg-sub-back" onClick={() => setSettingsView("list")}>
          ‹ 返回
        </button>
        <span>我的资料</span>
      </header>

      <div className="tg-profile-hero">
        <button
          type="button"
          className="profile-avatar-upload"
          disabled={!canUploadAvatar || avatarBusy}
          onClick={() => fileRef.current?.click()}
          title={canUploadAvatar ? "更换头像" : "需开启 IPFS 并连接链"}
        >
          <ProfileAvatar title={displayName} avatarCid={profile?.avatarCid} size="lg" />
          {canUploadAvatar && (
            <span className="profile-avatar-upload-badge">
              {avatarBusy ? "…" : "📷"}
            </span>
          )}
        </button>
        <input
          ref={fileRef}
          type="file"
          accept="image/*"
          hidden
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) void onAvatarSelected(f);
            e.target.value = "";
          }}
        />

        {editing ? (
          <div className="tg-profile-edit">
            <input
              className="tg-profile-input"
              value={nick}
              onChange={(e) => setNick(e.target.value)}
              autoFocus
            />
            <button
              type="button"
              className="tg-profile-save"
              disabled={nickBusy}
              onClick={() => void saveNick()}
            >
              {nickBusy ? "保存中…" : "保存"}
            </button>
          </div>
        ) : (
          <>
            <h2 className="tg-profile-name">{displayName}</h2>
            <button type="button" className="tg-profile-edit-btn" onClick={() => setEditing(true)}>
              编辑昵称
            </button>
          </>
        )}
        <p className="tg-profile-bio">NexChat · 端到端加密即时通讯</p>
        {profileLoading && <p className="wx-shop-order-sub">同步链上资料…</p>}
        {error && <p className="wx-market-tx-status error">{error}</p>}
        {!canUploadAvatar && !config.useMock && (
          <p className="wx-shop-order-sub">头像需 IPFS 与链上账户（VITE_IPFS_ENABLED=true）</p>
        )}
      </div>

      <section className="tg-profile-section">
        <h3>账户</h3>
        <Row label="链上地址" value={shortAddress(displayAddr, 12, 8)} mono copy={displayAddr} />
        <Row label="钱包来源" value={source === "dev" ? `Dev (${config.devSeed})` : "桌面 keyring (SS58 273)"} />
        <Row
          label="链上 KeyPackage"
          value={
            config.useMock
              ? "Mock"
              : chainKeyPackageLoading
                ? "查询中…"
                : chainKeyPackageCount == null
                  ? "—"
                  : String(chainKeyPackageCount)
          }
        />
        <Row
          label="本地 KeyPackage"
          value={String(account?.keyPackagesAvailable ?? 0)}
        />
        <Row label="平台禁言" value={account?.platformMuted ? "是" : "否"} />
        {profile?.avatarCid && (
          <Row label="头像 CID" value={profile.avatarCid} mono />
        )}
      </section>

      <section className="tg-profile-section">
        <h3>加密状态</h3>
        <Row label="MLS 后端" value={config.mlsBackend} />
        <Row label="控制面" value={config.mlsControlPlane} />
        <Row
          label="Demo 群"
          value={mls?.ready ? `群 #${mlsGroupId ?? config.mlsDemoGroupId} · ${mls.role}` : "握手中…"}
        />
        <Row label="1:1 会话" value={`${directReady} 条 E2EE 就绪`} />
        <Row label="投递令牌" value={config.deliveryTokensEnabled ? "已启用" : "关闭"} />
      </section>
    </main>
  );
}

function Row({
  label,
  value,
  mono,
  copy,
}: {
  label: string;
  value: string;
  mono?: boolean;
  copy?: string;
}) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    if (!copy) return;
    const ok = await copyToClipboard(copy);
    if (!ok) return;
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="tg-profile-row">
      <span className="tg-profile-row-label">{label}</span>
      <span className={`tg-profile-row-value${mono ? " mono" : ""}`}>
        {value}
        {copy && (
          <button
            type="button"
            className="tg-copy-btn"
            title={copied ? "已复制" : "复制"}
            aria-label={copied ? "已复制" : "复制"}
            onClick={() => void handleCopy()}
          >
            {copied ? "✓" : "📋"}
          </button>
        )}
      </span>
    </div>
  );
}
