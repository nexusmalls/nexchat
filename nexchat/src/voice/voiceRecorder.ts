// EN: Browser MediaRecorder helper for hold-to-talk voice clips.
// CN: 按住说话用的浏览器 MediaRecorder 辅助类。

import { Capacitor } from "@capacitor/core";

export const MIN_VOICE_MS = 800;
export const MAX_VOICE_MS = 60_000;

const MIME_CANDIDATES = [
  "audio/webm;codecs=opus",
  "audio/webm",
  "audio/mp4",
  "audio/aac",
  "audio/ogg;codecs=opus",
  "audio/ogg",
];

export function voiceRecordingSupported(): boolean {
  if (typeof navigator === "undefined") return false;
  if (
    typeof Capacitor !== "undefined" &&
    Capacitor.isNativePlatform?.() &&
    Capacitor.getPlatform?.() === "android"
  ) {
    return true;
  }
  return (
    typeof MediaRecorder !== "undefined" &&
    !!navigator.mediaDevices?.getUserMedia
  );
}

export function preferredVoiceMime(): string {
  if (typeof MediaRecorder === "undefined") return "";
  for (const mime of MIME_CANDIDATES) {
    if (MediaRecorder.isTypeSupported(mime)) return mime;
  }
  return "";
}

function extForMime(mime: string): string {
  if (mime.includes("mp4") || mime.includes("aac")) return "m4a";
  if (mime.includes("ogg")) return "ogg";
  return "webm";
}

export class VoiceRecorder {
  private recorder: MediaRecorder | null = null;
  private stream: MediaStream | null = null;
  private chunks: Blob[] = [];
  private startedAt = 0;
  private maxTimer: ReturnType<typeof setTimeout> | null = null;

  get recording(): boolean {
    return this.recorder?.state === "recording";
  }

  elapsedMs(): number {
    if (!this.recording || !this.startedAt) return 0;
    return Date.now() - this.startedAt;
  }

  async start(onMaxDuration?: () => void): Promise<void> {
    if (this.recording) return;
    const mime = preferredVoiceMime();
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
    });
    this.chunks = [];
    this.recorder = mime
      ? new MediaRecorder(this.stream, { mimeType: mime })
      : new MediaRecorder(this.stream);
    this.recorder.ondataavailable = (e) => {
      if (e.data.size > 0) this.chunks.push(e.data);
    };
    this.recorder.start(200);
    this.startedAt = Date.now();
    if (this.maxTimer) clearTimeout(this.maxTimer);
    this.maxTimer = setTimeout(() => {
      if (this.recording) onMaxDuration?.();
    }, MAX_VOICE_MS);
  }

  async stop(cancel: boolean): Promise<{ file: File | null; reason?: "cancel" | "short" | "empty" }> {
    const rec = this.recorder;
    if (!rec || rec.state === "inactive") {
      this.cleanup();
      return { file: null, reason: "empty" };
    }
    const duration = Date.now() - this.startedAt;
    return new Promise((resolve) => {
      rec.onstop = () => {
        if (this.maxTimer) clearTimeout(this.maxTimer);
        this.maxTimer = null;
        const mime = rec.mimeType || preferredVoiceMime() || "audio/webm";
        const blob = new Blob(this.chunks, { type: mime });
        this.cleanup();
        if (cancel) {
          resolve({ file: null, reason: "cancel" });
          return;
        }
        if (duration < MIN_VOICE_MS || blob.size < 64) {
          resolve({ file: null, reason: "short" });
          return;
        }
        const ext = extForMime(mime);
        resolve({
          file: new File([blob], `voice-${Date.now()}.${ext}`, { type: mime }),
        });
      };
      try {
        rec.stop();
      } catch {
        this.cleanup();
        resolve({ file: null, reason: "empty" });
      }
    });
  }

  cancel(): void {
    void this.stop(true);
  }

  private cleanup(): void {
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    this.recorder = null;
    this.chunks = [];
    this.startedAt = 0;
  }
}
