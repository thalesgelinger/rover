use crate::build_script::archive;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const SKIP_AUTO_BUILD_ENV: &str = "ROVER_WEB_SKIP_AUTO_BUILD";
const WASM_CRATE_RELATIVE_MANIFEST: &str = "rover-web-wasm/Cargo.toml";
const WASM_LINKER_RELATIVE_PATH: &str = "rover-web-wasm/scripts/emcc-linker.sh";
const WASM_TARGET_TRIPLE: &str = "wasm32-unknown-emscripten";
const NESTED_TARGET_DIR: &str = "target/rover-web-wasm-build";

struct BuildContext {
    workspace_root: PathBuf,
    cargo_bin: String,
    release: bool,
    nested_target_dir: PathBuf,
    linker_path: PathBuf,
}

pub fn auto_build_archive(out_tar: &Path) -> Result<(), String> {
    if env::var(SKIP_AUTO_BUILD_ENV).is_ok() {
        return Err("ROVER_WEB_SKIP_AUTO_BUILD set".to_string());
    }

    let ctx = BuildContext::from_env()?;
    run_wasm_build(&ctx)?;

    let (wasm_js, wasm_bin) = wasm_output_paths(&ctx);
    ensure_exists(&wasm_js, "wasm js output")?;
    ensure_exists(&wasm_bin, "wasm binary output")?;

    archive::write_runtime_archive(out_tar, &wasm_js, &wasm_bin).map_err(|e| e.to_string())
}

impl BuildContext {
    fn from_env() -> Result<Self, String> {
        let manifest_dir =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
        let workspace_root = manifest_dir
            .parent()
            .ok_or("failed to resolve workspace root")?
            .to_path_buf();
        let release = env::var("PROFILE").map_err(|e| e.to_string())? == "release";
        let cargo_bin = env::var("CARGO").map_err(|e| e.to_string())?;
        let nested_target_dir = workspace_root.join(NESTED_TARGET_DIR);
        let linker_path = workspace_root.join(WASM_LINKER_RELATIVE_PATH);

        ensure_exists(&linker_path, "linker script")?;

        Ok(Self {
            workspace_root,
            cargo_bin,
            release,
            nested_target_dir,
            linker_path,
        })
    }

    fn wasm_manifest_path(&self) -> PathBuf {
        self.workspace_root.join(WASM_CRATE_RELATIVE_MANIFEST)
    }

    fn profile_name(&self) -> &'static str {
        if self.release { "release" } else { "debug" }
    }
}

fn run_wasm_build(ctx: &BuildContext) -> Result<(), String> {
    eprintln!(
        "rover-cli build.rs: running cargo build -p rover-web-wasm --target {WASM_TARGET_TRIPLE}{}",
        if ctx.release { " --release" } else { "" }
    );

    let mut cmd = Command::new(&ctx.cargo_bin);
    cmd.current_dir(&ctx.workspace_root)
        .env("RUSTFLAGS", "-C panic=abort")
        .env("CARGO_PROFILE_RELEASE_PANIC", "abort")
        .env("CARGO_PROFILE_DEV_PANIC", "abort")
        .env("CARGO_TARGET_DIR", &ctx.nested_target_dir)
        .env(
            "CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER",
            &ctx.linker_path,
        )
        .arg("build")
        .arg("--manifest-path")
        .arg(ctx.wasm_manifest_path())
        .arg("--target")
        .arg(WASM_TARGET_TRIPLE);

    if ctx.release {
        cmd.arg("--release");
    }

    let status = cmd.status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "cargo build rover-web-wasm failed with status {status}"
        ));
    }

    Ok(())
}

fn wasm_output_paths(ctx: &BuildContext) -> (PathBuf, PathBuf) {
    let wasm_dir = ctx
        .nested_target_dir
        .join(WASM_TARGET_TRIPLE)
        .join(ctx.profile_name());
    let wasm_js = wasm_dir.join("rover_web_wasm.js");
    let wasm_bin = wasm_dir.join("rover_web_wasm.wasm");
    (wasm_js, wasm_bin)
}

fn ensure_exists(path: &Path, label: &str) -> Result<(), String> {
    if path.exists() {
        Ok(())
    } else {
        Err(format!("missing {label}: {}", path.display()))
    }
}
