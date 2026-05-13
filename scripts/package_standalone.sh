#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <target-triple> <binary-name> <out-dir>"
  exit 1
fi

target="$1"
binary="$2"
out_dir="$3"
profile_dir="target/${target}/release"

rm -rf "$out_dir"
mkdir -p "$out_dir/runtimes/macos" "$out_dir/runtimes/ios" "$out_dir/runtimes/android/arm64-v8a"

cp "$profile_dir/$binary" "$out_dir/$binary"
cp README.md "$out_dir/README.md"

if [[ -f "target/release/rover-macos-host" ]]; then
  cp "target/release/rover-macos-host" "$out_dir/runtimes/macos/rover-macos-host"
  chmod 755 "$out_dir/runtimes/macos/rover-macos-host"
elif [[ -f "target/debug/rover-macos-host" ]]; then
  cp "target/debug/rover-macos-host" "$out_dir/runtimes/macos/rover-macos-host"
  chmod 755 "$out_dir/runtimes/macos/rover-macos-host"
fi

for dylib in \
  "$profile_dir/librover_macos.dylib" \
  "target/release/librover_macos.dylib" \
  "target/debug/librover_macos.dylib"
do
  if [[ -f "$dylib" ]]; then
    cp "$dylib" "$out_dir/runtimes/macos/librover_macos.dylib"
    break
  fi
done

if [[ -f "target/ios/librover_ios.a" ]]; then
  cp "target/ios/librover_ios.a" "$out_dir/runtimes/ios/librover_ios.a"
fi

if [[ -f "target/ios/liblua5.4.a" ]]; then
  cp "target/ios/liblua5.4.a" "$out_dir/runtimes/ios/liblua5.4.a"
fi

if [[ -f "target/android/librover_android.so" ]]; then
  cp "target/android/librover_android.so" "$out_dir/runtimes/android/arm64-v8a/librover_android.so"
fi

find "$out_dir/runtimes" -type d -empty -delete
