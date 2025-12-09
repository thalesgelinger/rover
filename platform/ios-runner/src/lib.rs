use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};

const BUILD_ROOT: &str = ".rover/build/ios-sim";
const VENDOR_XCCLI: &str = "platform/ios-runner/vendor/XcodeProjectCLI";

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

    pub fn generate_project(&self) -> Result<()> {
        std::fs::create_dir_all(&self.build_dir).context("create build dir")?;
        // TODO: copy template into build dir and patch plist/targets via xcodeprojectcli
        Ok(())
    }

    pub fn build_and_run_sim(&self) -> Result<()> {
        self.build_swift_tool()?;
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
