import { useEffect, useRef, useState } from "react";
import { decodeQrFromImageData, QrScanError } from "@/wallet/qrScanner";

interface QrCameraModalProps {
  open: boolean;
  onClose: () => void;
  onScan: (raw: string) => void;
}

// EN: Live camera QR scanner (browser getUserMedia + jsQR).
// CN: 实时相机扫码（浏览器 getUserMedia + jsQR）。
export function QrCameraModal({ open, onClose, onScan }: QrCameraModalProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    if (!open) {
      setError(null);
      setStarting(false);
      return;
    }

    let stream: MediaStream | null = null;
    let raf = 0;
    let cancelled = false;

    const start = async () => {
      setStarting(true);
      setError(null);
      try {
        if (!navigator.mediaDevices?.getUserMedia) {
          throw new QrScanError("CAMERA_DENIED", "当前环境不支持相机");
        }
        stream = await navigator.mediaDevices.getUserMedia({
          video: { facingMode: { ideal: "environment" } },
          audio: false,
        });
        if (cancelled) return;

        const video = videoRef.current;
        const canvas = canvasRef.current;
        if (!video || !canvas) return;

        video.srcObject = stream;
        await video.play();

        const context = canvas.getContext("2d");
        if (!context) return;

        const tick = () => {
          if (cancelled || !video.videoWidth || !video.videoHeight) {
            raf = requestAnimationFrame(tick);
            return;
          }

          canvas.width = video.videoWidth;
          canvas.height = video.videoHeight;
          context.drawImage(video, 0, 0, canvas.width, canvas.height);

          try {
            const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
            const raw = decodeQrFromImageData(imageData);
            if (raw) {
              onScan(raw);
              onClose();
              return;
            }
          } catch {
            /* keep scanning */
          }

          raf = requestAnimationFrame(tick);
        };

        raf = requestAnimationFrame(tick);
      } catch (e) {
        if (cancelled) return;
        if (e instanceof QrScanError) {
          setError(e.message);
        } else if (e instanceof DOMException && e.name === "NotAllowedError") {
          setError("请允许使用相机后重试");
        } else {
          setError(e instanceof Error ? e.message : "无法打开相机");
        }
      } finally {
        if (!cancelled) setStarting(false);
      }
    };

    void start();

    return () => {
      cancelled = true;
      cancelAnimationFrame(raf);
      stream?.getTracks().forEach((track) => track.stop());
      if (videoRef.current) videoRef.current.srcObject = null;
    };
  }, [open, onClose, onScan]);

  if (!open) return null;

  return (
    <div className="wx-wallet-modal-backdrop wx-qr-camera-backdrop" onClick={onClose}>
      <div
        className="wx-wallet-modal wx-qr-camera-modal"
        role="dialog"
        aria-modal
        onClick={(e) => e.stopPropagation()}
      >
        <header className="wx-wallet-modal-head">
          <h3>相机扫码</h3>
          <button type="button" className="wx-wallet-modal-close" onClick={onClose}>
            ✕
          </button>
        </header>
        <div className="wx-wallet-modal-body wx-qr-camera-body">
          {error ? (
            <p className="wx-market-tx-status error">{error}</p>
          ) : (
            <>
              <div className="wx-qr-camera-frame">
                <video ref={videoRef} className="wx-qr-camera-video" playsInline muted />
                <canvas ref={canvasRef} className="wx-qr-camera-canvas" aria-hidden />
              </div>
              <p className="wx-wallet-modal-hint">
                {starting ? "正在启动相机…" : "将收款二维码对准取景框"}
              </p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
