use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};

const BUILD_ROOT: &str = ".rover/build/ios-sim";
const VENDOR_XCCLI: &str = "platform/ios-runner/vendor/XcodeProjectCLI";
const BUILD_BIN: &str = "platform/ios-runner/vendor/XcodeProjectCLI/.build/debug/xcodeprojectcli";

pub struct IosRunner {
    build_dir: PathBuf,
}

impl IosRunner {
    pub fn new() -> Self {
        Self {
            build_dir: PathBuf::from(BUILD_ROOT),
        }
    }

    pub fn ensure_prereqs(&self) -> Result<()> {
        check_cmd("xcrun")?;
        check_cmd("swift")?;
        Ok(())
    }

    pub fn stage_payload(&self, entry: &Path) -> Result<()> {
        let entry = entry.canonicalize().with_context(|| format!("canonicalize {}", entry.display()))?;
        let assets_dir = entry.parent().map(|p| p.join("assets"));

        let app_dir = self.build_dir.join("app");
        fs::create_dir_all(&app_dir).context("create app dir")?;

        let target_entry = app_dir.join("main.lua");
        fs::copy(&entry, &target_entry)
            .with_context(|| format!("copy entry to {}", target_entry.display()))?;

        if let Some(assets) = assets_dir {
            if assets.exists() {
                let dest = app_dir.join("assets");
                if dest.exists() {
                    fs::remove_dir_all(&dest).context("clean old assets")?;
                }
                fs::create_dir_all(&dest).context("create assets dest")?;
                copy_dir(&assets, &dest)?;
            }
        }

        Ok(())
    }

    pub fn generate_project(&self) -> Result<()> {
        fs::create_dir_all(&self.build_dir).context("create build dir")?;
        self.build_swift_tool()?;
        // TODO: copy template into build dir and patch plist/targets via xcodeprojectcli
        Ok(())
    }

    pub fn build_and_run_sim(&self, entry: &Path) -> Result<()> {
        self.stage_payload(entry)?;
        self.generate_project()?;
        // TODO: build Rust staticlib, bundle Lua/assets, launch simctl
        Ok(())
    }

    fn build_swift_tool(&self) -> Result<()> {
        let xcc_path = Path::new(VENDOR_XCCLI);
        if !xcc_path.exists() {
            return Err(anyhow!("XcodeProjectCLI vendor missing at {}", xcc_path.display()));
        }
        let status = Command::new("swift")
            .arg("build")
            .current_dir(xcc_path)
            .status()
            .context("swift build XcodeProjectCLI")?;
        if !status.success() {
            return Err(anyhow!("swift build failed"));
        }
        Ok(())
    }
}

fn check_cmd(cmd: &str) -> Result<()> {
    let status = Command::new("/usr/bin/which")
        .arg(cmd)
        .status()
        .with_context(|| format!("which {cmd}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{cmd} not found"))
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src).with_context(|| format!("read_dir {}", src.display()))? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            fs::create_dir_all(&dst_path)?;
            copy_dir(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}
