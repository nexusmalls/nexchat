package com.nexus.nexchat;

import android.Manifest;
import android.media.MediaRecorder;
import android.util.Base64;
import com.getcapacitor.JSObject;
import com.getcapacitor.PermissionState;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.CapacitorPlugin;
import com.getcapacitor.annotation.Permission;
import com.getcapacitor.annotation.PermissionCallback;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;

@CapacitorPlugin(
    name = "NativeVoice",
    permissions = {
        @Permission(strings = { Manifest.permission.RECORD_AUDIO }, alias = "microphone")
    }
)
public class NativeVoicePlugin extends Plugin {

    private MediaRecorder recorder;
    private File outputFile;
    private long startedAt;
    private PluginCall pendingStartCall;

    @PluginMethod
    public void checkSupport(PluginCall call) {
        if (getPermissionState("microphone") != PermissionState.GRANTED) {
            pendingStartCall = call;
            requestPermissionForAlias("microphone", call, "warmupPermCallback");
            return;
        }
        JSObject ret = new JSObject();
        ret.put("supported", true);
        call.resolve(ret);
    }

    @PermissionCallback
    private void warmupPermCallback(PluginCall call) {
        PluginCall target = pendingStartCall != null ? pendingStartCall : call;
        pendingStartCall = null;
        JSObject ret = new JSObject();
        ret.put("supported", getPermissionState("microphone") == PermissionState.GRANTED);
        target.resolve(ret);
    }

    @PluginMethod
    public void startRecording(PluginCall call) {
        if (recorder != null) {
            call.reject("ALREADY_RECORDING");
            return;
        }
        if (getPermissionState("microphone") != PermissionState.GRANTED) {
            pendingStartCall = call;
            requestPermissionForAlias("microphone", call, "microphonePermCallback");
            return;
        }
        startRecordingInternal(call);
    }

    @PermissionCallback
    private void microphonePermCallback(PluginCall call) {
        PluginCall startCall = pendingStartCall != null ? pendingStartCall : call;
        pendingStartCall = null;
        if (getPermissionState("microphone") != PermissionState.GRANTED) {
            startCall.reject("PERMISSION_DENIED");
            return;
        }
        startRecordingInternal(startCall);
    }

    private void startRecordingInternal(PluginCall call) {
        try {
            outputFile = File.createTempFile("nexchat-voice-", ".m4a", getContext().getCacheDir());
            recorder = new MediaRecorder();
            recorder.setAudioSource(MediaRecorder.AudioSource.MIC);
            recorder.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4);
            recorder.setAudioEncoder(MediaRecorder.AudioEncoder.AAC);
            recorder.setAudioSamplingRate(44100);
            recorder.setAudioEncodingBitRate(96000);
            recorder.setOutputFile(outputFile.getAbsolutePath());
            recorder.prepare();
            recorder.start();
            startedAt = System.currentTimeMillis();
            call.resolve();
        } catch (Exception e) {
            cleanupRecording();
            call.reject("FAILED_TO_START", e.getMessage());
        }
    }

    @PluginMethod
    public void stopRecording(PluginCall call) {
        if (recorder == null) {
            call.reject("NOT_RECORDING");
            return;
        }
        long durationMs = System.currentTimeMillis() - startedAt;
        try {
            recorder.stop();
        } catch (Exception e) {
            cleanupRecording();
            call.reject("FAILED_TO_STOP", e.getMessage());
            return;
        }
        File file = outputFile;
        cleanupRecording();
        if (file == null || !file.exists()) {
            call.reject("FAILED_TO_READ", "Recording file missing");
            return;
        }
        try {
            byte[] bytes = readFileBytes(file);
            file.delete();
            JSObject ret = new JSObject();
            ret.put("base64", Base64.encodeToString(bytes, Base64.NO_WRAP));
            ret.put("mimeType", "audio/mp4");
            ret.put("durationMs", durationMs);
            call.resolve(ret);
        } catch (IOException e) {
            call.reject("FAILED_TO_READ", e.getMessage());
        }
    }

    @PluginMethod
    public void cancelRecording(PluginCall call) {
        if (recorder != null) {
            try {
                recorder.stop();
            } catch (Exception ignored) {
                // EN: stop() may throw on very short clips — still discard.
            }
        }
        if (outputFile != null) {
            outputFile.delete();
        }
        cleanupRecording();
        call.resolve();
    }

    private void cleanupRecording() {
        if (recorder != null) {
            try {
                recorder.release();
            } catch (Exception ignored) {
            }
        }
        recorder = null;
        outputFile = null;
        startedAt = 0;
    }

    private static byte[] readFileBytes(File file) throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream((int) file.length());
        byte[] buf = new byte[8192];
        try (FileInputStream in = new FileInputStream(file)) {
            int n;
            while ((n = in.read(buf)) >= 0) {
                out.write(buf, 0, n);
            }
        }
        return out.toByteArray();
    }
}
