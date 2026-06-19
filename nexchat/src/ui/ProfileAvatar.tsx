import { ipfsClient } from "@/ipfs/ipfsClient";

// EN: User profile avatar — IPFS image or letter fallback.
// CN: 用户资料头像——IPFS 图片或首字母占位。
export function ProfileAvatar({
  title,
  avatarCid,
  className = "",
  size = "lg",
}: {
  title: string;
  avatarCid?: string | null;
  className?: string;
  size?: "sm" | "lg";
}) {
  const letter = title[0]?.toUpperCase() ?? "?";
  const imgUrl = avatarCid ? ipfsClient.gatewayUrl(avatarCid) : null;
  const showImg = !!imgUrl;
  const sizeClass = size === "sm" ? "profile-avatar-sm" : "profile-avatar-lg";

  return (
    <div className={`profile-avatar ${sizeClass} ${className}`.trim()}>
      {showImg ? (
        <img
          className="profile-avatar-img"
          src={imgUrl!}
          alt=""
          loading="lazy"
          onError={(e) => {
            (e.target as HTMLImageElement).style.display = "none";
          }}
        />
      ) : null}
      {!showImg && <span className="profile-avatar-letter">{letter}</span>}
    </div>
  );
}
