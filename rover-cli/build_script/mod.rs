mod archive;
mod embedded;
mod web_wasm;

use std::env;
use std::fs;
use std::path::PathBuf;

const PREPACKAGED_ENV: &str = "ROVER_WEB_ASSETS_TAR_GZ";
const SKIP_AUTO_BUILD_ENV: &str = "ROVER_WEB_SKIP_AUTO_BUILD";

pub fn run() {
    emit_rerun_instructions();
    println!("cargo:warning=rover-cli build.rs: preparing embedded web assets");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    let out_tar = out_dir.join("rover_web_assets.tar.gz");

    if let Ok(path) = env::var(PREPACKAGED_ENV) {
        let src = PathBuf::from(path);
        println!(
            "cargo:warning=rover-cli build.rs: using prepackaged web assets {}",
            src.display()
        );
        println!("cargo:rerun-if-changed={}", src.display());
        fs::copy(&src, &out_tar).expect("failed to copy ROVER_WEB_ASSETS_TAR_GZ");
        return;
    }

    if env::var(SKIP_AUTO_BUILD_ENV).is_ok() {
        println!("cargo:warning=rover-cli build.rs: using placeholder web assets");
        archive::write_placeholder_archive(&out_tar)
            .expect("failed to write placeholder web assets");
        return;
    }

    println!("cargo:warning=rover-cli build.rs: auto-building rover-web-wasm");
    if let Err(err) = web_wasm::auto_build_archive(&out_tar) {
        panic!("web assets auto-build failed: {err}");
    }
    println!("cargo:warning=rover-cli build.rs: embedded runtime web assets ready");
}

fn emit_rerun_instructions() {
    println!("cargo:rerun-if-env-changed={PREPACKAGED_ENV}");
    println!("cargo:rerun-if-env-changed={SKIP_AUTO_BUILD_ENV}");

    let watched = [
        "../rover-web-wasm/src/lib.rs",
        "../rover-web-wasm/build.rs",
        "../rover-web-wasm/scripts/emcc-linker.sh",
        "build_script/assets/runtime_index.html",
        "build_script/assets/runtime_loader.js",
    ];

    for path in watched {
        println!("cargo:rerun-if-changed={path}");
    }
}
