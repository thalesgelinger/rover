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
const pullHtml = module.cwrap('rover_web_pull_html', 'string', ['number']);
const dispatchClick = module.cwrap('rover_web_dispatch_click', 'number', ['number', 'number']);

let luaPtr = 0;
try {
  luaPtr = init();
} catch (err) {
  print(`[fatal] init failed: ${String(err)}`);
  throw err;
}
const source = await fetch('./app.lua').then((r) => r.text());
let status = -1;
try {
  status = loadLua(luaPtr, source);
  if (status !== 0) {
    print(`lua load failed: ${status}`);
  }
} catch (err) {
  print(`[fatal] loadLua crashed: ${String(err)}`);
  throw err;
}

let prevHtml = '';

function syncDom() {
  const html = pullHtml(luaPtr) || '';
  if (app && html !== prevHtml) {
    app.innerHTML = html;
    prevHtml = html;
    bindButtons();
  }
}

function tickAndSync() {
  try {
    const tickStatus = tick(luaPtr);
    if (tickStatus !== 0) {
      print(`tick failed: ${tickStatus}`);
    }
  } catch (err) {
    print(`[fatal] tick crashed: ${String(err)}`);
    throw err;
  }
  syncDom();
}

function bindButtons() {
  const buttons = document.querySelectorAll('[data-rid]');
  buttons.forEach((btn) => {
    btn.addEventListener('click', () => {
      const id = Number(btn.getAttribute('data-rid'));
      if (!Number.isNaN(id)) {
        dispatchClick(luaPtr, id);
        syncDom();
      }
    });
  });
}

tickAndSync();
EOF

tar -C "$tmp_dir" -czf "$out_file" index.html loader.js rover_web_wasm.js rover_web_wasm.wasm
echo "wrote $out_file"
