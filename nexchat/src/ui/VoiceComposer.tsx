// EN: WeChat-style hold-to-talk bar for mobile chat composer.
// CN: 手机聊天输入栏——微信式按住说话。

import { useEffect, useRef, useState } from "react";
import {
  isMicrophonePermissionError,
  requestNativeMicrophone,
} from "@/capacitor/appSettings";
import {
  cancelNativeVoiceRecording,
  isNativeVoicePermissionError,
  isNativeVoicePlatform,
  isNativeVoiceUnavailable,
  nativeVoiceResultToFile,
  startNativeVoiceRecording,
  stopNativeVoiceRecording,
  warmUpNativeVoice,
} from "@/capacitor/nativeVoice";
import { PermissionPromptBar } from "@/ui/PermissionPromptBar";
import {
  MAX_VOICE_MS,
  MIN_VOICE_MS,
  VoiceRecorder,
  voiceRecordingSupported,
} from "@/voice/voiceRecorder";

const CANCEL_DRAG_PX = 56;
const APK_DOWNLOAD_URL = "https://nexusmall.net/nexchat/download.html";

export interface VoiceComposerProps {
  onSend: (file: File) => Promise<void>;
  onError?: (message: string) => void;
  disabled?: boolean;
}

type VoiceBlockReason = "permission" | "update" | null;

export function VoiceComposer({ onSend, onError, disabled }: VoiceComposerProps) {
  const recorderRef = useRef<VoiceRecorder | null>(null);
  const startYRef = useRef(0);
  const holdActiveRef = useRef(false);
  const startingRef = useRef(false);
  const nativeActiveRef = useRef(false);
  const openedSettingsRef = useRef(false);
  const nativeStartAtRef = useRef(0);
  const [phase, setPhase] = useState<"idle" | "recording" | "cancel">("idle");
  const [elapsed, setElapsed] = useState(0);
  const [busy, setBusy] = useState(false);
  const [blockReason, setBlockReason] = useState<VoiceBlockReason>(null);

  const androidNative = isNativeVoicePlatform();
  const supported = voiceRecordingSupported();

  useEffect(() => {
    if (androidNative) return;
    recorderRef.current = new VoiceRecorder();
    return () => {
      recorderRef.current?.cancel();
    };
  }, [androidNative]);

  useEffect(() => {
    if (!androidNative) return;
    void warmUpNativeVoice();
  }, [androidNative]);

  useEffect(() => {
    if (phase !== "recording") {
      setElapsed(0);
      return;
    }
    const id = setInterval(() => {
      if (nativeActiveRef.current) {
        setElapsed(Date.now() - nativeStartAtRef.current);
        return;
      }
      setElapsed(recorderRef.current?.elapsedMs() ?? 0);
    }, 200);
    return () => clearInterval(id);
  }, [phase]);

  useEffect(() => {
    if (phase !== "recording") return;
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = prev;
    };
  }, [phase]);

  useEffect(() => {
    const onVisible = () => {
      if (document.visibilityState === "visible" && openedSettingsRef.current) {
        openedSettingsRef.current = false;
        setBlockReason(null);
        if (androidNative) void warmUpNativeVoice();
      }
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, [androidNative]);

  const finishNative = async (cancel: boolean) => {
    if (!nativeActiveRef.current) {
      setPhase("idle");
      return;
    }
    setBusy(true);
    nativeActiveRef.current = false;
    try {
      if (cancel) {
        await cancelNativeVoiceRecording();
        return;
      }
      const result = await stopNativeVoiceRecording();
      if (result.durationMs < MIN_VOICE_MS) {
        onError?.("说话时间太短");
        return;
      }
      setBlockReason(null);
      await onSend(nativeVoiceResultToFile(result));
    } catch (e) {
      if (isNativeVoicePermissionError(e)) {
        setBlockReason("permission");
        return;
      }
      onError?.(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setPhase("idle");
    }
  };

  const finishWeb = async (cancel: boolean) => {
    const rec = recorderRef.current;
    if (!rec?.recording) {
      setPhase("idle");
      return;
    }
    setBusy(true);
    try {
      const { file, reason } = await rec.stop(cancel);
      if (file) {
        setBlockReason(null);
        await onSend(file);
      } else if (reason === "short") {
        onError?.("说话时间太短");
      }
    } catch (e) {
      onError?.(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setPhase("idle");
    }
  };

  const finish = async (cancel: boolean) => {
    if (nativeActiveRef.current) {
      await finishNative(cancel);
      return;
    }
    await finishWeb(cancel);
  };

  const cancelPendingStart = async () => {
    if (nativeActiveRef.current) {
      nativeActiveRef.current = false;
      await cancelNativeVoiceRecording();
      return;
    }
    recorderRef.current?.cancel();
  };

  const onHoldStart = (clientY: number) => {
    if (disabled || busy || !supported || phase !== "idle") return;
    holdActiveRef.current = true;
    startingRef.current = true;
    startYRef.current = clientY;
    void (async () => {
      try {
        setBlockReason(null);

        if (androidNative) {
          try {
            await startNativeVoiceRecording();
            if (!holdActiveRef.current) {
              await cancelNativeVoiceRecording();
              return;
            }
            nativeActiveRef.current = true;
            nativeStartAtRef.current = Date.now();
            setPhase("recording");
            return;
          } catch (e) {
            if (isNativeVoicePermissionError(e)) {
              setBlockReason("permission");
              return;
            }
            if (isNativeVoiceUnavailable(e)) {
              setBlockReason("update");
              return;
            }
            onError?.(e instanceof Error ? e.message : "无法启动录音");
            return;
          }
        }

        const native = await requestNativeMicrophone();
        if (native === "denied") {
          setBlockReason("permission");
          return;
        }
        await recorderRef.current?.start(() => void finish(false));
        if (!holdActiveRef.current) {
          await cancelPendingStart();
          return;
        }
        setPhase("recording");
      } catch (e) {
        if (isMicrophonePermissionError(e)) {
          setBlockReason("permission");
          return;
        }
        onError?.(e instanceof Error ? e.message : "无法启动录音");
      } finally {
        startingRef.current = false;
      }
    })();
  };

  const onHoldMove = (clientY: number) => {
    if (phase !== "recording") return;
    setPhase(startYRef.current - clientY >= CANCEL_DRAG_PX ? "cancel" : "recording");
  };

  const onHoldEnd = () => {
    holdActiveRef.current = false;
    if (startingRef.current) {
      void (async () => {
        await cancelPendingStart();
        startingRef.current = false;
      })();
      return;
    }
    if (phase === "idle") return;
    void finish(phase === "cancel");
  };

  if (!supported) {
    return (
      <div className="wx-voice-unsupported">
        当前浏览器不支持语音消息，请用 📎 发送音频文件
      </div>
    );
  }

  const sec = Math.min(MAX_VOICE_MS / 1000, Math.ceil(elapsed / 1000));
  const label =
    phase === "cancel" ? "松开 取消" : phase === "recording" ? "松开发送" : "按住 说话";

  return (
    <>
      {blockReason === "permission" && (
        <PermissionPromptBar
          message="请在系统设置中允许 NexChat 使用麦克风"
          actionLabel="去设置"
          onDismiss={() => setBlockReason(null)}
          onBeforeOpenSettings={() => {
            openedSettingsRef.current = true;
          }}
        />
      )}
      {blockReason === "update" && (
        <PermissionPromptBar
          message="语音消息需要安装最新版 NexChat"
          actionLabel="去下载"
          onDismiss={() => setBlockReason(null)}
          onAction={() => {
            window.open(APK_DOWNLOAD_URL, "_blank", "noopener,noreferrer");
          }}
        />
      )}
      <button
        type="button"
        className={`wx-voice-hold-btn${phase !== "idle" ? " active" : ""}${phase === "cancel" ? " cancel" : ""}`}
        disabled={disabled || busy}
        onContextMenu={(e) => e.preventDefault()}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          onHoldStart(e.clientY);
        }}
        onPointerMove={(e) => onHoldMove(e.clientY)}
        onPointerUp={(e) => {
          try {
            e.currentTarget.releasePointerCapture(e.pointerId);
          } catch {
            /* already released */
          }
          onHoldEnd();
        }}
        onPointerCancel={() => onHoldEnd()}
      >
        {busy ? "发送中…" : label}
      </button>

      {phase !== "idle" && (
        <div className="wx-voice-recording-overlay" aria-live="polite">
          <div className={`wx-voice-recording-panel${phase === "cancel" ? " cancel" : ""}`}>
            <div className="wx-voice-recording-icon">{phase === "cancel" ? "✕" : "🎙"}</div>
            <p className="wx-voice-recording-hint">
              {phase === "cancel" ? "松开手指，取消发送" : "上滑取消 · 松开发送"}
            </p>
            <p className="wx-voice-recording-time">{sec}s</p>
          </div>
        </div>
      )}
    </>
  );
}
