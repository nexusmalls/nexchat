// EN: Conversation avatar — IPFS image with letter / 「群」 fallback.
// CN: 会话头像——IPFS 图片，失败时回退首字母 / 「群」占位。

import { useEffect, useMemo, useState } from "react";
import { ipfsUrls } from "@/ipfs/ipfsClient";
import type { ConvKind } from "@/types/viewModels";

export function Avatar({
  kind,
  title,
  avatarCid,
  className = "",
}: {
  kind: ConvKind;
  title: string;
  avatarCid?: string | null;
  className?: string;
}) {
  const [imgIndex, setImgIndex] = useState(0);
  const [imgFailed, setImgFailed] = useState(false);
  const urls = useMemo(
    () => (avatarCid ? ipfsUrls(avatarCid) : []),
    [avatarCid],
  );
  useEffect(() => {
    setImgIndex(0);
    setImgFailed(false);
  }, [avatarCid]);
  const imgUrl = !imgFailed && urls.length > 0 ? urls[imgIndex] : null;
  const fallback = kind === "group" ? "群" : (title[0]?.toUpperCase() ?? "?");

  return (
    <div className={`avatar ${kind} ${className}`.trim()}>
      {imgUrl ? (
        <img
          key={imgUrl}
          className="avatar-img"
          src={imgUrl}
          alt=""
          loading="lazy"
          onError={() => {
            if (imgIndex + 1 < urls.length) {
              setImgIndex((i) => i + 1);
              return;
            }
            setImgFailed(true);
          }}
        />
      ) : (
        fallback
      )}
    </div>
  );
}
