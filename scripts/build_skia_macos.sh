#!/usr/bin/env bash
set -euo pipefail

# Build Skia m120 CPU-only with C API for macOS arm64/x64.
# Produces vendor/skia/macos-{arch}/{lib,include} with libskia.a and include/c headers.
# Requires: Xcode CLT, Python3, git, curl, depot_tools in PATH, ~20GB disk, ~30min.

SKIA_TAG="${SKIA_TAG:-chrome/m120}"
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SKIA_DIR="${ROOT_DIR}/.skia-src"
OUT_DIR_BASE="${ROOT_DIR}/vendor/skia"
NINJA_BIN="${NINJA_BIN:-ninja}"

fetch_skia() {
  if [ -d "${SKIA_DIR}" ]; then
    echo "[skia] reuse ${SKIA_DIR}";
    (cd "${SKIA_DIR}" && git fetch --tags origin && git checkout "${SKIA_TAG}" && python3 tools/git-sync-deps)
    return;
  fi
  mkdir -p "${SKIA_DIR}"
  git clone https://github.com/google/skia "${SKIA_DIR}"
  (cd "${SKIA_DIR}" && git checkout "${SKIA_TAG}")
  (cd "${SKIA_DIR}" && python3 tools/git-sync-deps)
}

build_arch() {
  local arch="$1"; shift
  local gn_out="${SKIA_DIR}/out/macos-${arch}"
  local vendor_arch="macos-${arch}"
  local args=(
    "is_official_build=true"
    "is_component_build=false"
    "skia_use_gl=false"
    "skia_use_metal=false"
    "skia_use_system_libpng=false"
    "skia_use_system_libjpeg_turbo=false"
    "skia_use_system_libwebp=false"
    "skia_use_system_zlib=true"
    "skia_use_icu=false"
    "skia_enable_skottie=false"
    "skia_enable_svg=false"
    "skia_enable_pdf=false"
    "target_cpu=\"${arch}\""
  )

  local ninja_flags="${NINJA_FLAGS:--j1}"
  local ninja_bin="${NINJA_BIN}"
  if ! command -v "${ninja_bin}" >/dev/null 2>&1; then
    echo "[skia] ninja not found: ${ninja_bin}" >&2
    exit 1
  fi

  echo "[skia] gn gen ${gn_out} (${arch})"
  (cd "${SKIA_DIR}" && PATH="${SKIA_DIR}/bin:${ROOT_DIR}/.depot_tools:$PATH" gn gen "${gn_out}" --args="${args[*]}")

  echo "[skia] ninja ${arch} ${ninja_flags}"
  (cd "${SKIA_DIR}" && PATH="${SKIA_DIR}/bin:${ROOT_DIR}/.depot_tools:$PATH" "${ninja_bin}" ${ninja_flags} -C "${gn_out}" skia)

  mkdir -p "${OUT_DIR_BASE}/${vendor_arch}/lib" "${OUT_DIR_BASE}/${vendor_arch}/include"
  cp "${gn_out}/libskia.a" "${OUT_DIR_BASE}/${vendor_arch}/lib/"
  rsync -a "${SKIA_DIR}/include/" "${OUT_DIR_BASE}/${vendor_arch}/include/"
}

main() {
  fetch_skia
  build_arch "arm64"
  build_arch "x64"
  echo "[skia] done -> ${OUT_DIR_BASE}/macos-{arm64,x64}"
}

main "$@"
