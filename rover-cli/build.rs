use flate2::write::GzEncoder;
use flate2::Compression;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tar::{Builder, Header};

fn main() {
    println!("cargo:rerun-if-env-changed=ROVER_WEB_ASSETS_TAR_GZ");
    println!("cargo:rerun-if-env-changed=ROVER_WEB_SKIP_AUTO_BUILD");
    println!("cargo:rerun-if-env-changed=ROVER_WEB_FORCE_FALLBACK");
    println!("cargo:warning=rover-cli build.rs: preparing embedded web assets");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    let out_tar = out_dir.join("rover_web_assets.tar.gz");

    if let Ok(path) = env::var("ROVER_WEB_ASSETS_TAR_GZ") {
        let src = PathBuf::from(path);
        println!(
            "cargo:warning=rover-cli build.rs: using prepackaged web assets {}",
            src.display()
        );
        println!("cargo:rerun-if-changed={}", src.display());
        fs::copy(&src, &out_tar).expect("failed to copy ROVER_WEB_ASSETS_TAR_GZ");
        return;
    }

    let force_fallback = env::var("ROVER_WEB_FORCE_FALLBACK").is_ok();
    if !force_fallback {
        println!("cargo:warning=rover-cli build.rs: auto-building rover-web-wasm");
        match auto_build_archive(&out_tar) {
            Ok(()) => {
                println!("cargo:warning=rover-cli build.rs: embedded runtime web assets ready");
                return;
            }
            Err(err) => {
                let profile = env::var("PROFILE").unwrap_or_else(|_| "dev".to_string());
                let skip_auto = env::var("ROVER_WEB_SKIP_AUTO_BUILD").is_ok();
                if profile == "release" && !skip_auto {
                    panic!("web assets auto-build failed in release: {err}");
                }
                println!("cargo:warning=web assets auto-build failed: {err}");
                println!("cargo:warning=using fallback embedded web assets");
            }
        }
    }

    write_fallback_archive(&out_tar).expect("failed to write fallback web assets archive");
}

fn auto_build_archive(out_tar: &Path) -> Result<(), String> {
    if env::var("ROVER_WEB_SKIP_AUTO_BUILD").is_ok() {
        return Err("ROVER_WEB_SKIP_AUTO_BUILD set".to_string());
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let workspace_root = manifest_dir
        .parent()
        .ok_or("failed to resolve workspace root")?
        .to_path_buf();

    let profile = env::var("PROFILE").map_err(|e| e.to_string())?;
    let release = profile == "release";
    let cargo = env::var("CARGO").map_err(|e| e.to_string())?;
    let nested_target_dir = workspace_root.join("target/rover-web-wasm-build");

    let linker_path = workspace_root.join("rover-web-wasm/scripts/emcc-linker.sh");
    if !linker_path.exists() {
        return Err(format!("missing linker script: {}", linker_path.display()));
    }

    println!(
        "cargo:warning=rover-cli build.rs: running cargo build -p rover-web-wasm --target wasm32-unknown-emscripten{}",
        if release { " --release" } else { "" }
    );

    let mut cmd = Command::new(cargo);
    cmd.current_dir(&workspace_root)
        .env("RUSTFLAGS", "-C panic=abort")
        .env("CARGO_PROFILE_RELEASE_PANIC", "abort")
        .env("CARGO_PROFILE_DEV_PANIC", "abort")
        .env("CARGO_TARGET_DIR", &nested_target_dir)
        .env("CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER", linker_path)
        .arg("build")
        .arg("--manifest-path")
        .arg(workspace_root.join("rover-web-wasm/Cargo.toml"))
        .arg("--target")
        .arg("wasm32-unknown-emscripten");

    if release {
        cmd.arg("--release");
    }

    let status = cmd.status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "cargo build rover-web-wasm failed with status {status}"
        ));
    }

    let wasm_profile = if release { "release" } else { "debug" };
    let wasm_dir = nested_target_dir
        .join("wasm32-unknown-emscripten")
        .join(wasm_profile);
    let wasm_js = wasm_dir.join("rover_web_wasm.js");
    let wasm_bin = wasm_dir.join("rover_web_wasm.wasm");

    if !wasm_js.exists() {
        return Err(format!("missing wasm js output: {}", wasm_js.display()));
    }
    if !wasm_bin.exists() {
        return Err(format!(
            "missing wasm binary output: {}",
            wasm_bin.display()
        ));
    }

    write_runtime_archive(out_tar, &wasm_js, &wasm_bin).map_err(|e| e.to_string())
}

fn write_runtime_archive(out_path: &Path, wasm_js: &Path, wasm_bin: &Path) -> std::io::Result<()> {
    let file = fs::File::create(out_path)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);

    append_str(&mut tar, "index.html", runtime_index_html())?;
    append_str(&mut tar, "loader.js", runtime_loader_js())?;
    tar.append_path_with_name(wasm_js, "rover_web_wasm.js")?;
    tar.append_path_with_name(wasm_bin, "rover_web_wasm.wasm")?;

    let encoder = tar.into_inner()?;
    let mut file = encoder.finish()?;
    file.flush()?;
    Ok(())
}

fn write_fallback_archive(out_path: &Path) -> std::io::Result<()> {
    let file = fs::File::create(out_path)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);

    append_str(&mut tar, "index.html", fallback_index_html())?;
    append_str(&mut tar, "loader.js", fallback_loader_js())?;
    append_str(&mut tar, "rover_web.js", fallback_rover_web_js())?;

    let encoder = tar.into_inner()?;
    let mut file = encoder.finish()?;
    file.flush()?;
    Ok(())
}

fn append_str<W: Write>(tar: &mut Builder<W>, path: &str, content: &str) -> std::io::Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, path, content.as_bytes())?;
    Ok(())
}

fn fallback_index_html() -> &'static str {
    r#"<!doctype html>
<html lang=\"en\">
<head>
  <meta charset=\"utf-8\">
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
  <title>Rover Web</title>
</head>
<body>
  <main>
    <h1>Rover Web</h1>
    <pre id=\"out\"></pre>
  </main>
  <script type=\"module\" src=\"./loader.js\"></script>
</body>
</html>
"#
}

fn fallback_loader_js() -> &'static str {
    r#"const out = document.getElementById('out');

function log(msg) {
  if (!out) return;
  out.textContent += `${msg}\n`;
}

async function boot() {
  try {
    const mod = await import('./rover_web.js');
    if (typeof mod.default !== 'function') {
      log('embedded fallback assets active');
      log('release assets missing: set ROVER_WEB_ASSETS_TAR_GZ at build time');
      return;
    }
    log('loaded rover_web.js');
  } catch (err) {
    log(`failed loading web runtime: ${err}`);
  }
}

boot();
"#
}

fn fallback_rover_web_js() -> &'static str {
    r#"export default null;
"#
}

fn runtime_index_html() -> &'static str {
    r#"<!doctype html>
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
"#
}

fn runtime_loader_js() -> &'static str {
    r#"import createModule from './rover_web_wasm.js';

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

const luaPtr = init();
const source = await fetch('./app.lua').then((r) => r.text());
const status = loadLua(luaPtr, source);
if (status !== 0) {
  print(`lua load failed: ${status}`);
}

let prevHtml = '';

function render() {
  tick(luaPtr);
  const html = pullHtml(luaPtr) || '';
  if (app && html !== prevHtml) {
    app.innerHTML = html;
    prevHtml = html;
    bindButtons();
  }
}

function bindButtons() {
  const buttons = document.querySelectorAll('[data-rid]');
  buttons.forEach((btn) => {
    btn.addEventListener('click', () => {
      const id = Number(btn.getAttribute('data-rid'));
      if (!Number.isNaN(id)) {
        dispatchClick(luaPtr, id);
        render();
      }
    });
  });
}

render();
"#
}
