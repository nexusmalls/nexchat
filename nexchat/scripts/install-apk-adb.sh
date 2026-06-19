#!/usr/bin/env bash
# EN: Install the debug APK onto the first (or all) online adb device/emulator.
# CN: 将 debug APK 安装到第一个（或全部）在线 adb 设备/模拟器。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APK="${APK:-$ROOT/android/app/build/outputs/apk/debug/app-debug.apk}"
INSTALL_ALL=false
SERIAL=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

  -a, --all          Install to every online device (default: first only)
  -s, --serial ID    Target a specific serial (e.g. emulator-5554)
  -p, --apk PATH     APK path (default: android/app/build/outputs/apk/debug/app-debug.apk)
  -h, --help         Show this help

Examples:
  $(basename "$0")
  $(basename "$0") --all
  $(basename "$0") -s emulator-5556
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -a | --all) INSTALL_ALL=true; shift ;;
    -s | --serial) SERIAL="$2"; shift 2 ;;
    -p | --apk) APK="$2"; shift 2 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

if [[ ! -f "$APK" ]]; then
  echo "APK not found: $APK" >&2
  echo "Build first: cd $ROOT/android && ./gradlew assembleDebug" >&2
  exit 1
fi

pick_devices() {
  adb devices | awk 'NR > 1 && $2 == "device" { print $1 }'
}

wait_for_any_device() {
  local i
  for i in $(seq 1 60); do
    if pick_devices | grep -q .; then
      return 0
    fi
    echo "waiting for adb device... ($i/60)"
    sleep 3
  done
  echo "No adb device appeared within 3 minutes." >&2
  adb devices -l >&2 || true
  exit 1
}

wait_boot_completed() {
  local id="$1"
  adb -s "$id" wait-for-device
  adb -s "$id" shell 'while [ "$(getprop sys.boot_completed)" != "1" ]; do sleep 2; done; echo boot_ok'
}

install_one() {
  local id="$1"
  echo "==> target: $id"
  wait_boot_completed "$id"
  adb -s "$id" install -r "$APK"
  adb -s "$id" shell am start -n com.nexus.nexchat/.MainActivity 2>/dev/null \
    || adb -s "$id" shell monkey -p com.nexus.nexchat -c android.intent.category.LAUNCHER 1 \
    || true
}

wait_for_any_device

if [[ -n "$SERIAL" ]]; then
  if ! adb devices | awk -v s="$SERIAL" '$1 == s && $2 == "device" { found=1 } END { exit !found }'; then
    echo "Device not online: $SERIAL" >&2
    adb devices -l >&2
    exit 1
  fi
  install_one "$SERIAL"
  exit 0
fi

mapfile -t DEVICES < <(pick_devices)
if [[ ${#DEVICES[@]} -eq 0 ]]; then
  echo "No online adb devices." >&2
  exit 1
fi

if "$INSTALL_ALL"; then
  for id in "${DEVICES[@]}"; do
    install_one "$id"
  done
else
  echo "Online devices: ${DEVICES[*]}"
  install_one "${DEVICES[0]}"
fi

echo "Done."
