// EN: Mobile chat attachment sheet — album / camera photo / camera video / any file.
// CN: 手机聊天附件面板——相册 / 拍照 / 拍视频 / 任意文件。

import { useRef, type ChangeEvent, type RefObject } from "react";

export interface ChatAttachSheetProps {
  open: boolean;
  onClose: () => void;
  onFile: (file: File) => void;
}

export function ChatAttachSheet({ open, onClose, onFile }: ChatAttachSheetProps) {
  const galleryRef = useRef<HTMLInputElement>(null);
  const photoRef = useRef<HTMLInputElement>(null);
  const videoRef = useRef<HTMLInputElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const onInputChange = (e: ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    e.target.value = "";
    if (!f) return;
    onClose();
    onFile(f);
  };

  // EN: Click input BEFORE close — closing must not unmount pickers (see hidden inputs below).
  // CN: 先 click 再 close——关闭时不能卸载 file input。
  const openPicker = (ref: RefObject<HTMLInputElement | null>) => {
    ref.current?.click();
    onClose();
  };

  return (
    <>
      {open && (
        <div className="dm-overlay tg-modal-overlay" onClick={onClose}>
          <aside className="wx-action-sheet" onClick={(e) => e.stopPropagation()}>
            <button type="button" className="wx-action-row" onClick={() => openPicker(galleryRef)}>
              <span className="wx-action-icon">🖼</span>
              <span className="wx-action-label">相册</span>
            </button>
            <button type="button" className="wx-action-row" onClick={() => openPicker(photoRef)}>
              <span className="wx-action-icon">📷</span>
              <span className="wx-action-label">拍照</span>
            </button>
            <button type="button" className="wx-action-row" onClick={() => openPicker(videoRef)}>
              <span className="wx-action-icon">🎬</span>
              <span className="wx-action-label">拍视频</span>
            </button>
            <button type="button" className="wx-action-row" onClick={() => openPicker(fileRef)}>
              <span className="wx-action-icon">📎</span>
              <span className="wx-action-label">文件</span>
            </button>
            <button type="button" className="wx-action-cancel" onClick={onClose}>
              取消
            </button>
          </aside>
        </div>
      )}

      <input
        ref={galleryRef}
        type="file"
        hidden
        accept="image/*,video/*"
        onChange={onInputChange}
      />
      <input
        ref={photoRef}
        type="file"
        hidden
        accept="image/*"
        capture="environment"
        onChange={onInputChange}
      />
      <input
        ref={videoRef}
        type="file"
        hidden
        accept="video/*"
        capture="environment"
        onChange={onInputChange}
      />
      <input ref={fileRef} type="file" hidden onChange={onInputChange} />
    </>
  );
}
