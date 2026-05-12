#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/target/debug/rover-macos-host}"

cargo build --manifest-path "$ROOT/Cargo.toml" -p rover-macos

swiftc \
  "$ROOT/rover-macos/swift/RoverMacosHost.swift" \
  "$ROOT/rover-macos/swift/main.swift" \
  -framework AppKit \
  -L "$ROOT/target/debug" \
  -lrover_macos \
  -Xlinker -rpath \
  -Xlinker "$ROOT/target/debug" \
  -Xlinker -rpath \
  -Xlinker @executable_path \
  -o "$OUT"

echo "$OUT"
