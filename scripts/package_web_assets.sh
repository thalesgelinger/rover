#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <rover_web.js> <rover_web.wasm> <out.tar.gz>"
  exit 1
fi

js_file="$1"
wasm_file="$2"
out_file="$3"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

export COPYFILE_DISABLE=1

cp "$js_file" "$tmp_dir/rover_web_wasm.js"
cp "$wasm_file" "$tmp_dir/rover_web_wasm.wasm"

cat > "$tmp_dir/index.html" <<'EOF'
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Rover Web</title>
</head>
<body>
  <div id="app"></div>
  <script type="module" src="./loader.js"></script>
</body>
</html>
EOF

cat > "$tmp_dir/loader.js" <<'EOF'
import createModule from './rover_web_wasm.js';

const app = document.getElementById('app');
const print = (msg) => {
  if (!app) return;
  app.textContent += `${msg}\n`;
};

const module = await createModule({
  locateFile: (p) => `./${p}`,
  print: (t) => print(String(t)),
  printErr: (t) => print(`[stderr] ${String(t)}`),
});

const init = module.cwrap('rover_web_init', 'number', []);
const loadLua = module.cwrap('rover_web_load_lua', 'number', ['number', 'string']);
const tick = module.cwrap('rover_web_tick', 'number', ['number']);

const luaPtr = init();
const source = await fetch('./app.lua').then((r) => r.text());
const status = loadLua(luaPtr, source);
if (status !== 0) {
  print(`lua load failed: ${status}`);
}

setInterval(() => tick(luaPtr), 16);
EOF

tar -C "$tmp_dir" -czf "$out_file" index.html loader.js rover_web_wasm.js rover_web_wasm.wasm
echo "wrote $out_file"
