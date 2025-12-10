use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::Value as JsonValue;

const BUILD_ROOT: &str = ".rover/build/ios-sim";
const BUNDLE_ID: &str = "dev.rover.app";
const IOS_SIM_TARGET: &str = "aarch64-apple-ios-sim";

pub struct IosRunner {
    build_dir: PathBuf,
}

impl IosRunner {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            build_dir: cwd.join(BUILD_ROOT),
        }
    }

    pub fn ensure_prereqs(&self) -> Result<()> {
        check_cmd("xcrun")?;
        check_cmd("swift")?;
        Ok(())
    }

    pub fn stage_payload(&self, entry: &Path) -> Result<()> {
        let entry = entry
            .canonicalize()
            .with_context(|| format!("canonicalize {}", entry.display()))?;
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

    pub fn generate_project(&self) -> Result<PathBuf> {
        fs::create_dir_all(&self.build_dir).context("create build dir")?;
        let template = Path::new("platform/ios-runner/vendor/XcodeProjectCLI/Templates/ios-empty");
        let out = self.build_dir.join("project");
        if out.exists() {
            fs::remove_dir_all(&out).context("clean old project")?;
        }
        if !template.exists() {
            return Err(anyhow!("template missing at {}", template.display()));
        }
        fs::create_dir_all(&out).context("create project dir")?;
        copy_dir(template, &out)
            .with_context(|| format!("copy template from {}", template.display()))?;

        let proj_dir = out.join("RoverApp.xcodeproj");
        fs::create_dir_all(&proj_dir).context("create xcodeproj dir")?;
        let pbx_src = template.join("project.pbxproj");
        let pbx_target = proj_dir.join("project.pbxproj");
        fs::copy(&pbx_src, &pbx_target)
            .with_context(|| format!("copy pbxproj {}", pbx_src.display()))?;

        Ok(out)
    }

    pub fn build_and_run_sim(&self, entry: &Path) -> Result<()> {
        if self.build_dir.exists() {
            fs::remove_dir_all(&self.build_dir).ok();
        }
        self.stage_payload(entry)?;
        let lib = self.build_rust_staticlib()?;
        let project_dir = self.generate_project()?;
        self.place_staticlib(&project_dir, &lib)?;
        let device = select_sim_device()?;
        boot_device(&device)?;
        let app_path = build_app(&project_dir, &device, &self.build_dir)?;
        bundle_payload(&self.build_dir, &app_path)?;
        install_and_launch(&device, &app_path)?;
        Ok(())
    }

    fn build_rust_staticlib(&self) -> Result<PathBuf> {
        let sdk = sim_sdk_path()?;
        let cc = "/usr/bin/clang";
        let ar = "/usr/bin/ar";
        let status = Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg("rover-runtime")
            .arg("--target")
            .arg(IOS_SIM_TARGET)
            .env("MACOSX_DEPLOYMENT_TARGET", "16.0")
            .env("CC_aarch64-apple-ios-sim", cc)
            .env("AR_aarch64-apple-ios-sim", ar)
            .env(
                "CFLAGS_aarch64-apple-ios-sim",
                format!(
                    "-isysroot {} -arch arm64 -mios-simulator-version-min=16.0",
                    sdk
                ),
            )
            .env(
                "LDFLAGS_aarch64-apple-ios-sim",
                format!(
                    "-isysroot {} -arch arm64 -mios-simulator-version-min=16.0",
                    sdk
                ),
            )
            .status()
            .context("cargo build rover-runtime (ios sim)")?;
        if !status.success() {
            return Err(anyhow!("cargo build rover-runtime failed"));
        }
        let out = Path::new("target")
            .join(IOS_SIM_TARGET)
            .join("debug/librover_runtime.a");
        if !out.exists() {
            return Err(anyhow!("missing staticlib at {}", out.display()));
        }
        Ok(out)
    }

    fn place_staticlib(&self, project_dir: &Path, lib: &Path) -> Result<()> {
        let dest = project_dir.join("librover_runtime.a");
        fs::copy(lib, &dest)
            .with_context(|| format!("copy {} to {}", lib.display(), dest.display()))?;
        Ok(())
    }
}

fn sim_sdk_path() -> Result<String> {
    let output = Command::new("xcrun")
        .arg("--sdk")
        .arg("iphonesimulator")
        .arg("--show-sdk-path")
        .output()
        .context("xcrun --show-sdk-path")?;
    if !output.status.success() {
        return Err(anyhow!("xcrun --show-sdk-path failed"));
    }
    let sdk = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sdk.is_empty() {
        return Err(anyhow!("empty sdk path"));
    }
    Ok(sdk)
}

#[derive(Debug, Deserialize)]
struct SimDevice {
    udid: String,
    name: String,
    state: Option<String>,
    is_available: Option<bool>,
}

fn select_sim_device() -> Result<SimDevice> {
    let output = Command::new("xcrun")
        .arg("simctl")
        .arg("list")
        .arg("devices")
        .arg("-j")
        .output()
        .context("simctl list devices")?;
    if !output.status.success() {
        return Err(anyhow!("simctl list failed"));
    }
    let json: JsonValue = serde_json::from_slice(&output.stdout)?;
    let devices_obj = json
        .get("devices")
        .ok_or_else(|| anyhow!("missing devices"))?;
    let mut candidates: Vec<((u32, u32, u32), SimDevice)> = Vec::new();
    if let Some(map) = devices_obj.as_object() {
        for (runtime, list) in map {
            if !runtime.contains("iOS") {
                continue;
            }
            let version = parse_runtime_version(runtime);
            if let Some(arr) = list.as_array() {
                for item in arr {
                    let dev: SimDevice = serde_json::from_value(item.clone())?;
                    let available = dev.is_available.unwrap_or(true);
                    if available && dev.name.contains("iPhone") {
                        candidates.push((version, dev));
                    }
                }
            }
        }
    }
    candidates
        .into_iter()
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, d)| d)
        .ok_or_else(|| anyhow!("no available iOS simulator"))
}

fn parse_runtime_version(runtime: &str) -> (u32, u32, u32) {
    let mut parts = Vec::new();
    let mut current = String::new();
    for ch in runtime.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            parts.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    let major = parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

fn boot_device(dev: &SimDevice) -> Result<()> {
    if dev.state.as_deref() == Some("Booted") {
        return Ok(());
    }
    let status = Command::new("xcrun")
        .arg("simctl")
        .arg("boot")
        .arg(&dev.udid)
        .status()
        .context("simctl boot")?;
    if !status.success() {
        // ignore if already booted
        println!(
            "[rover][ios] sim boot exited with status {:?}",
            status.code()
        );
    }
    Ok(())
}

fn build_app(project_dir: &Path, dev: &SimDevice, build_dir: &Path) -> Result<PathBuf> {
    let derived = build_dir.join("DerivedData");
    if derived.exists() {
        fs::remove_dir_all(&derived).ok();
    }
    let status = Command::new("xcodebuild")
        .current_dir(project_dir)
        .arg("-project")
        .arg("RoverApp.xcodeproj")
        .arg("-scheme")
        .arg("RoverApp")
        .arg("-configuration")
        .arg("Debug")
        .arg("-sdk")
        .arg("iphonesimulator")
        .arg("-destination")
        .arg(format!("id={}", dev.udid))
        .arg("-derivedDataPath")
        .arg(&derived)
        .status()
        .context("xcodebuild")?;
    if !status.success() {
        return Err(anyhow!("xcodebuild failed"));
    }
    let app = derived.join("Build/Products/Debug-iphonesimulator/RoverApp.app");
    if !app.exists() {
        return Err(anyhow!("built app missing at {}", app.display()));
    }
    Ok(app)
}

fn bundle_payload(build_dir: &Path, app: &Path) -> Result<()> {
    let payload = build_dir.join("app");
    if !payload.exists() {
        return Ok(());
    }
    let target = app.join("rover");
    if target.exists() {
        fs::remove_dir_all(&target).ok();
    }
    fs::create_dir_all(&target)?;
    copy_dir(&payload, &target)
}

fn install_and_launch(dev: &SimDevice, app: &Path) -> Result<()> {
    let status = Command::new("xcrun")
        .arg("simctl")
        .arg("install")
        .arg(&dev.udid)
        .arg(app)
        .status()
        .context("simctl install")?;
    if !status.success() {
        return Err(anyhow!("simctl install failed"));
    }

    let status = Command::new("xcrun")
        .arg("simctl")
        .arg("launch")
        .arg(&dev.udid)
        .arg(BUNDLE_ID)
        .status()
        .context("simctl launch")?;
    if !status.success() {
        return Err(anyhow!("simctl launch failed"));
    }
    Ok(())
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
