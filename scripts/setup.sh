#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DEPOT_TOOLS_DIR="${ROOT_DIR}/.depot_tools"
SKIA_DIR="${ROOT_DIR}/.skia-src"
DEFAULT_TAG="chrome/m120"

say() { echo "[setup] $*"; }

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    say "missing $1; install via: $2"; exit 1; fi
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  say "macOS required for this setup"; exit 1; fi

if ! xcode-select -p >/dev/null 2>&1; then
  say "Xcode command line tools not installed. Run: xcode-select --install"; exit 1; fi

require_cmd git "brew install git"
require_cmd curl "brew install curl"
require_cmd python3 "brew install python"

# depot_tools
if [[ ! -d "${DEPOT_TOOLS_DIR}" ]]; then
  say "cloning depot_tools";
  git clone https://chromium.googlesource.com/chromium/tools/depot_tools "${DEPOT_TOOLS_DIR}"
else
  say "depot_tools present";
fi

# skia source (shallow) for tool scripts
if [[ ! -d "${SKIA_DIR}" ]]; then
  say "cloning skia (${DEFAULT_TAG}) shallow";
  git clone --branch "${DEFAULT_TAG}" --depth 1 https://github.com/google/skia "${SKIA_DIR}" || {
    say "shallow clone failed; retry without --depth";
    git clone https://github.com/google/skia "${SKIA_DIR}";
    (cd "${SKIA_DIR}" && git checkout "${DEFAULT_TAG}")
  }
else
  say "skia repo present";
fi

PATH_PREPEND="${SKIA_DIR}/bin:${DEPOT_TOOLS_DIR}:$PATH"

# fetch gn/ninja locally
if [[ ! -x "${SKIA_DIR}/bin/gn" ]]; then
  say "fetching gn";
  (cd "${SKIA_DIR}" && PATH="${PATH_PREPEND}" python3 bin/fetch-gn)
else
  say "gn present (${SKIA_DIR}/bin/gn)";
fi

if ! command -v ninja >/dev/null 2>&1; then
  if [[ -x "${SKIA_DIR}/bin/fetch-ninja" ]]; then
    say "fetching ninja";
    (cd "${SKIA_DIR}" && PATH="${PATH_PREPEND}" python3 bin/fetch-ninja)
  else
    say "ninja missing; install via brew install ninja"; exit 1;
  fi
else
  say "ninja present ($(command -v ninja))";
fi

say "done. Add to shell PATH (e.g. in your shell rc):"
cat <<EOF
export PATH="${SKIA_DIR}/bin:${DEPOT_TOOLS_DIR}:$PATH"
EOF
say "then build Skia: PATH=\"${SKIA_DIR}/bin:${DEPOT_TOOLS_DIR}:$PATH\" ./scripts/build_skia_macos.sh"
