#!/usr/bin/env bash
set -euo pipefail

mode="${1:-release}"
if [[ "$mode" != "release" && "$mode" != "debug" ]]; then
  echo "usage: $0 [release|debug]"
  exit 1
fi

if ! command -v emcc >/dev/null 2>&1; then
  echo "error: emcc not found in PATH"
  exit 1
fi

if [[ "$mode" == "release" ]]; then
  wasm_profile="release"
  release_flag="--release"
  bin_path="target/release/rover"
else
  wasm_profile="debug"
  release_flag=""
  bin_path="target/debug/rover"
fi

echo "[1/3] build wasm runtime"
linker_path="$(pwd)/rover-web-wasm/scripts/emcc-linker.sh"
if [[ -n "$release_flag" ]]; then
  RUSTFLAGS="-C panic=abort" CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER="$linker_path" cargo build -p rover-web-wasm --target wasm32-unknown-emscripten "$release_flag"
else
  RUSTFLAGS="-C panic=abort" CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER="$linker_path" cargo build -p rover-web-wasm --target wasm32-unknown-emscripten
fi

wasm_dir="target/wasm32-unknown-emscripten/${wasm_profile}"
wasm_js="${wasm_dir}/rover_web_wasm.js"
wasm_bin="${wasm_dir}/rover_web_wasm.wasm"

if [[ ! -f "$wasm_js" ]]; then
  echo "error: missing $wasm_js"
  exit 1
fi

if [[ ! -f "$wasm_bin" ]]; then
  echo "error: missing $wasm_bin"
  exit 1
fi

tmp_tar="$(mktemp /tmp/rover-web-assets.XXXXXX)"
assets_tar="${tmp_tar}.tar.gz"
rm -f "$tmp_tar"

echo "[2/3] package web assets"
"$(dirname "$0")/package_web_assets.sh" "$wasm_js" "$wasm_bin" "$assets_tar"

echo "[3/3] build rover cli with embedded web assets"
if [[ -n "$release_flag" ]]; then
  ROVER_WEB_ASSETS_TAR_GZ="$assets_tar" cargo build -p rover_cli "$release_flag"
else
  ROVER_WEB_ASSETS_TAR_GZ="$assets_tar" cargo build -p rover_cli
fi

echo "ok: built ${bin_path}"
echo "run: ${bin_path} run main.lua -p web"
