use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::Value as JsonValue;

const BUILD_ROOT: &str = ".rover/build/ios-sim";
const VENDOR_XCCLI: &str = "platform/ios-runner/vendor/XcodeProjectCLI";
const BUNDLE_ID: &str = "dev.rover.app";

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

    pub fn generate_project(&self) -> Result<PathBuf> {
        fs::create_dir_all(&self.build_dir).context("create build dir")?;
        let xcc_bin = self.build_swift_tool()?;
        if !xcc_bin.exists() {
            return Err(anyhow!("xcodeprojectcli binary missing at {}", xcc_bin.display()));
        }


        let template = Path::new("platform/ios-runner/vendor/XcodeProjectCLI/Templates/ios-empty");
        let out = self.build_dir.join("project");
        if out.exists() {
            fs::remove_dir_all(&out).context("clean old project")?;
        }
        if !template.exists() {
            return Err(anyhow!("template missing at {}", template.display()));
        }
        let status = Command::new(&xcc_bin)
            .arg("--template")
            .arg(template)
            .arg("--out")
            .arg(&out)
            .status()
            .context("run xcodeprojectcli copy")?;
        if !status.success() {
            return Err(anyhow!("xcodeprojectcli copy failed"));
        }

        Ok(out)
    }

    pub fn build_and_run_sim(&self, entry: &Path) -> Result<()> {
        self.stage_payload(entry)?;
        let project_dir = self.generate_project()?;
        let device = select_sim_device()?;
        boot_device(&device)?;
        let app_path = build_app(&project_dir, &device, &self.build_dir)?;
        bundle_payload(&self.build_dir, &app_path)?;
        install_and_launch(&device, &app_path)?;
        Ok(())
    }

    fn build_swift_tool(&self) -> Result<PathBuf> {

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
        Ok(xcc_path.join(".build/debug/xcodeprojectcli"))
    }

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
    let mut chosen: Option<SimDevice> = None;
    if let Some(map) = devices_obj.as_object() {
        for (runtime, list) in map {
            if !runtime.contains("iOS") {
                continue;
            }
            if let Some(arr) = list.as_array() {
                for item in arr {
                    let dev: SimDevice = serde_json::from_value(item.clone())?;
                    let available = dev.is_available.unwrap_or(true);
                    if available && dev.name.contains("iPhone") {
                        chosen = Some(dev);
                        break;
                    }
                }
            }
            if chosen.is_some() {
                break;
            }
        }
    }
    chosen.ok_or_else(|| anyhow!("no available iOS simulator"))
}

fn boot_device(dev: &SimDevice) -> Result<()> {
    let status = Command::new("xcrun")
        .arg("simctl")
        .arg("boot")
        .arg(&dev.udid)
        .status()
        .context("simctl boot")?;
    if !status.success() {
        // ignore if already booted
        println!("[rover][ios] sim boot exited with status {:?}", status.code());
    }
    Ok(())
}

fn build_app(project_dir: &Path, dev: &SimDevice, build_dir: &Path) -> Result<PathBuf> {
    let derived = build_dir.join("DerivedData");
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
