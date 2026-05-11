#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${RUN_ID:-$$}"
TMP_ROOT="${TMPDIR:-/tmp}/rover-native-smoke-${RUN_ID}"
INSTALL_ROOT="$TMP_ROOT/install"
PROJECT_ROOT="$TMP_ROOT/project"
IOS_DEVICE="${IOS_DEVICE:-iPhone 17 Pro}"
APP_NAME="Rover Native Smoke"
IOS_BUNDLE="lu.rover.generated.nativesmoke"
ANDROID_PACKAGE="lu.rover.generated.nativesmoke"

if [[ "${ROVER_KEEP_SMOKE_TMP:-0}" != "1" ]]; then
  trap 'rm -rf "$TMP_ROOT"' EXIT
fi

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'missing required command: %s\n' "$1" >&2
    exit 1
  fi
}

first_android_serial() {
  adb devices | awk 'NR > 1 && $2 == "device" { print $1; exit }'
}

boot_android_if_needed() {
  local serial
  serial="$(first_android_serial)"
  if [[ -n "$serial" ]]; then
    printf '%s\n' "$serial"
    return
  fi

  local avd
  avd="$(emulator -list-avds | head -n 1)"
  if [[ -z "$avd" ]]; then
    printf 'no Android device and no AVD available\n' >&2
    exit 1
  fi

  emulator -avd "$avd" -no-snapshot-load -no-audio -no-boot-anim \
    >"$TMP_ROOT/android-emulator.log" 2>&1 &
  adb wait-for-device
  for _ in {1..120}; do
    if [[ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]]; then
      break
    fi
    sleep 1
  done
  first_android_serial
}

write_project() {
  mkdir -p "$PROJECT_ROOT"
  cat >"$PROJECT_ROOT/rover.lua" <<LUA
return {
  name = "$APP_NAME",
  ios = {
    bundle_id = "$IOS_BUNDLE",
  },
  android = {
    package_name = "$ANDROID_PACKAGE",
  },
}
LUA

  cat >"$PROJECT_ROOT/main.lua" <<'LUA'
local ui = rover.ui

function rover.render()
  local count = rover.signal(0)

  return ui.scroll_view {
    style = { width = "full", height = "full", bg_color = "#f8fafc" },
    ui.column {
      style = { padding = 24, gap = 12, width = "full", bg_color = "#f8fafc" },
      ui.text { "Rover smoke", style = { color = "#0f172a" } },
      ui.text { "Native cross-platform", style = { color = "#2563eb" } },
      ui.text { "Count: " .. count, style = { color = "#16a34a" } },
      ui.row {
        style = { gap = 8, width = "full" },
        ui.button {
          label = "Increment",
          style = { padding = 8, bg_color = "#dbeafe", border_color = "#60a5fa", border_width = 1, color = "#1d4ed8" },
          on_click = function()
            count.val = count.val + 1
          end,
        },
      },
      ui.column {
        style = { padding = 12, gap = 8, width = "full", bg_color = "#ecfeff", border_color = "#06b6d4", border_width = 1 },
        ui.text { "Styled native section", style = { color = "#0e7490" } },
        ui.text { "Background, border, text color", style = { color = "#475569" } },
      },
    },
  }
end
LUA
}

run_and_verify_ios() {
  (cd "$PROJECT_ROOT" && rover run main.lua --platform ios)
  agent-device --session rover-smoke-ios open "$IOS_BUNDLE" --platform ios --device "$IOS_DEVICE" >/dev/null
  agent-device --session rover-smoke-ios wait text "Rover smoke" 10000 >/dev/null
  agent-device --session rover-smoke-ios screenshot --out "$TMP_ROOT/ios.png" >/dev/null
  agent-device --session rover-smoke-ios close >/dev/null
}

run_and_verify_android() {
  local serial="$1"
  (cd "$PROJECT_ROOT" && rover run main.lua --platform android --device-id "$serial")
  agent-device --session rover-smoke-android open "$ANDROID_PACKAGE" \
    --platform android \
    --serial "$serial" \
    --activity "$ANDROID_PACKAGE/lu.rover.host.MainActivity" >/dev/null
  agent-device --session rover-smoke-android wait text "Rover smoke" 10000 >/dev/null
  agent-device --session rover-smoke-android screenshot --out "$TMP_ROOT/android.png" >/dev/null
  agent-device --session rover-smoke-android close >/dev/null
}

main() {
  require_cmd cargo
  require_cmd adb
  require_cmd emulator
  require_cmd agent-device

  rm -rf "$TMP_ROOT"
  mkdir -p "$INSTALL_ROOT"
  write_project

  ROVER_WEB_SKIP_AUTO_BUILD=1 cargo install \
    --path "$ROOT/rover-cli" \
    --debug \
    --force \
    --root "$INSTALL_ROOT"

  export PATH="$INSTALL_ROOT/bin:$PATH"
  export ROVER_SOURCE_ROOT="$ROOT"
  export ROVER_WEB_SKIP_AUTO_BUILD=1

  local serial
  serial="$(boot_android_if_needed)"
  if [[ -z "$serial" ]]; then
    printf 'failed to resolve Android serial\n' >&2
    exit 1
  fi

  run_and_verify_ios
  run_and_verify_android "$serial"

  printf 'ok: temp project %s\n' "$PROJECT_ROOT"
  printf 'ok: ios screenshot %s\n' "$TMP_ROOT/ios.png"
  printf 'ok: android screenshot %s\n' "$TMP_ROOT/android.png"
}

main "$@"
