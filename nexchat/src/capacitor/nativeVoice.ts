// EN: Android native hold-to-talk recording (bypasses flaky WebView getUserMedia).
// CN: Android 原生按住说话录音（绕过 WebView getUserMedia 不稳定问题）。

import { Capacitor, registerPlugin } from "@capacitor/core";

interface NativeVoiceStopResult {
  base64: string;
  mimeType: string;
  durationMs: number;
}

interface NativeVoicePlugin {
  checkSupport(): Promise<{ supported: boolean }>;
  startRecording(): Promise<void>;
  stopRecording(): Promise<NativeVoiceStopResult>;
  cancelRecording(): Promise<void>;
}

const NativeVoice = registerPlugin<NativeVoicePlugin>("NativeVoice");

let supportCache: boolean | null = null;

function pluginErrorCode(err: unknown): string {
  if (typeof err !== "object" || err === null) return "";
  const code = (err as { code?: string }).code;
  if (code) return code;
  const message = (err as { message?: string }).message ?? "";
  const match = /"([A-Z_]+)"/.exec(message);
  return match?.[1] ?? message;
}

export function isNativeVoicePlatform(): boolean {
  return Capacitor.isNativePlatform() && Capacitor.getPlatform() === "android";
}

export async function isNativeVoiceSupported(): Promise<boolean> {
  if (!isNativeVoicePlatform()) return false;
  if (supportCache !== null) return supportCache;
  try {
    const { supported } = await NativeVoice.checkSupport();
    supportCache = supported;
    return supported;
  } catch {
    supportCache = false;
    return false;
  }
}

/// EN: Pre-request Android mic permission when entering voice mode. CN: 进入语音模式时预申请麦克风。
export async function warmUpNativeVoice(): Promise<void> {
  if (!isNativeVoicePlatform()) return;
  try {
    await NativeVoice.checkSupport();
    supportCache = true;
  } catch {
    supportCache = false;
  }
}

export function isNativeVoicePermissionError(err: unknown): boolean {
  const code = pluginErrorCode(err);
  return code === "PERMISSION_DENIED" || code.includes("PERMISSION");
}

export function isNativeVoiceUnavailable(err: unknown): boolean {
  const code = pluginErrorCode(err).toLowerCase();
  return (
    code.includes("not implemented") ||
    code.includes("unimplemented") ||
    code === "plugin_not_implemented"
  );
}

export async function startNativeVoiceRecording(): Promise<void> {
  await NativeVoice.startRecording();
}

export async function stopNativeVoiceRecording(): Promise<NativeVoiceStopResult> {
  return NativeVoice.stopRecording();
}

export async function cancelNativeVoiceRecording(): Promise<void> {
  try {
    await NativeVoice.cancelRecording();
  } catch {
    /* already stopped */
  }
}

export function nativeVoiceResultToFile(result: NativeVoiceStopResult): File {
  const binary = atob(result.base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  const ext = result.mimeType.includes("mp4") ? "m4a" : "webm";
  return new File([bytes], `voice-${Date.now()}.${ext}`, { type: result.mimeType });
}
