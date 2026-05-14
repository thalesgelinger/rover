#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <rover_web.js> <rover_web.wasm> <out.tar.gz>"
  exit 1
fi

js_file="$1"
wasm_file="$2"
out_file="$3"

if [[ ! -s "$js_file" ]]; then
  echo "error: web runtime JS missing or empty: $js_file" >&2
  exit 1
fi

if [[ ! -s "$wasm_file" ]]; then
  echo "error: web runtime wasm missing or empty: $wasm_file" >&2
  exit 1
fi

if ! grep -q "export default\|export {" "$js_file"; then
  echo "error: web runtime JS is not an ES module factory: $js_file" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
script_dir="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$script_dir/.." && pwd)"

export COPYFILE_DISABLE=1

cp "$js_file" "$tmp_dir/rover_web_wasm.js"
cp "$wasm_file" "$tmp_dir/rover_web_wasm.wasm"
cp "$root/rover-cli/build_script/assets/runtime_index.html" "$tmp_dir/index.html"
cp "$root/rover-cli/build_script/assets/runtime_loader.js" "$tmp_dir/loader.js"

tar -C "$tmp_dir" -czf "$out_file" index.html loader.js rover_web_wasm.js rover_web_wasm.wasm
echo "wrote $out_file"
