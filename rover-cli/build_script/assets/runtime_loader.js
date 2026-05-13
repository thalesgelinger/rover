import * as wasmModule from './rover_web_wasm.js';

const createModule = wasmModule.default ?? wasmModule.Module ?? wasmModule;
if (typeof createModule !== 'function') {
  throw new Error('rover_web_wasm.js did not export an Emscripten module factory');
}

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
const nextWakeMs = module.cwrap('rover_web_next_wake_ms', 'number', ['number']);
const pullHtml = module.cwrap('rover_web_pull_html', 'string', ['number']);
const dispatchClick = module.cwrap('rover_web_dispatch_click', 'number', ['number', 'number']);
const dispatchInput = module.cwrap('rover_web_dispatch_input', 'number', ['number', 'number', 'string']);
const dispatchSubmit = module.cwrap('rover_web_dispatch_submit', 'number', ['number', 'number', 'string']);
const dispatchToggle = module.cwrap('rover_web_dispatch_toggle', 'number', ['number', 'number', 'number']);
const setViewport = module.cwrap('rover_web_set_viewport', 'number', ['number', 'number', 'number']);
const lastError = module.cwrap('rover_web_last_error', 'string', ['number']);

function describeStatus(status, phase) {
  const detail = (lastError(luaPtr) || '').trim();
  if (detail) {
    print(`[${phase}] ${detail}`);
    return;
  }
  print(`[${phase}] failed with status ${status}`);
}

const manifest = await fetch('./manifest.json').then((r) => r.json());

async function mountProjectFiles() {
  if (!module.FS_createPath || !module.FS_createDataFile) {
    return;
  }

  module.FS_createPath('/', 'project', true, true);
  const sourcePrefix = manifest.source_prefix || '/__rover_src';

  for (const relPath of manifest.files || []) {
    const urlPath = relPath.split('/').map((p) => encodeURIComponent(p)).join('/');
    const source = await fetch(`${sourcePrefix}/${urlPath}`).then((r) => r.text());
    const parts = relPath.split('/').filter(Boolean);
    let current = '/project';

    for (let i = 0; i < parts.length - 1; i++) {
      const dir = parts[i];
      module.FS_createPath(current, dir, true, true);
      current = `${current}/${dir}`;
    }

    const fileName = parts[parts.length - 1];
    module.FS_createDataFile(current, fileName, source, true, true, true);
  }
}

await mountProjectFiles();

let luaPtr = 0;
try {
  luaPtr = init();
} catch (err) {
  print(`[fatal] init failed: ${String(err)}`);
  throw err;
}
const sourcePrefix = manifest.source_prefix || '/__rover_src';
const entryUrl = (manifest.entry || '').split('/').map((p) => encodeURIComponent(p)).join('/');
const source = await fetch(`${sourcePrefix}/${entryUrl}`).then((r) => r.text());
let status = -1;
try {
  status = loadLua(luaPtr, source);
  if (status !== 0) {
    describeStatus(status, 'loadLua');
  }
} catch (err) {
  print(`[fatal] loadLua crashed: ${String(err)}`);
  throw err;
}

let prevHtml = '';
let wakeHandle = null;

function syncDom() {
  const html = pullHtml(luaPtr) || '';
  if (app && html !== prevHtml) {
    app.innerHTML = html;
    prevHtml = html;
    bindInteractiveNodes();
  }
}

function runStatus(status, phase) {
  if (status !== 0) {
    describeStatus(status, phase);
  }
}

function tickAndSync() {
  try {
    const tickStatus = tick(luaPtr);
    if (tickStatus !== 0) {
      describeStatus(tickStatus, 'tick');
    }
  } catch (err) {
    print(`[fatal] tick crashed: ${String(err)}`);
    throw err;
  }
  syncDom();
}

function scheduleNextFlush() {
  if (wakeHandle !== null) {
    clearTimeout(wakeHandle);
    wakeHandle = null;
  }

  let delay = -1;
  try {
    delay = nextWakeMs(luaPtr);
  } catch (err) {
    print(`[fatal] nextWakeMs crashed: ${String(err)}`);
    throw err;
  }

  if (delay < 0) {
    return;
  }

  wakeHandle = setTimeout(() => {
    wakeHandle = null;
    tickAndSync();
    scheduleNextFlush();
  }, Math.max(0, delay));
}

function bindButtons() {
  const nodes = document.querySelectorAll('[data-rid]');
  nodes.forEach((node) => {
    if (node.dataset.roverBound === '1') return;
    node.dataset.roverBound = '1';

    const id = Number(node.getAttribute('data-rid'));
    if (Number.isNaN(id)) return;

    if (node.tagName === 'BUTTON') {
      node.addEventListener('click', () => {
        runStatus(dispatchClick(luaPtr, id), 'click');
        syncDom();
        scheduleNextFlush();
      });
      return;
    }

    if (node.tagName === 'INPUT' && node.type === 'checkbox') {
      node.addEventListener('change', () => {
        runStatus(dispatchToggle(luaPtr, id, node.checked ? 1 : 0), 'toggle');
        syncDom();
        scheduleNextFlush();
      });
      return;
    }

    if (node.tagName === 'INPUT') {
      node.addEventListener('input', () => {
        runStatus(dispatchInput(luaPtr, id, node.value), 'input');
        syncDom();
        scheduleNextFlush();
      });
      node.addEventListener('keydown', (event) => {
        if (event.key !== 'Enter') return;
        runStatus(dispatchSubmit(luaPtr, id, node.value), 'submit');
        syncDom();
        scheduleNextFlush();
      });
    }
  });
}

function bindInteractiveNodes() {
  bindButtons();
}

function syncViewport() {
  runStatus(setViewport(luaPtr, window.innerWidth || 1, window.innerHeight || 1), 'viewport');
  syncDom();
  scheduleNextFlush();
}

tickAndSync();
syncViewport();
window.addEventListener('resize', syncViewport);
scheduleNextFlush();
