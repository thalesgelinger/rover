use crate::abi::HostCallbacks;
use crate::renderer::IosRenderer;
use anyhow::{Context, Result};
use rover_ui::app::App;
use rover_ui::events::UiEvent;
use rover_ui::ui::NodeId;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

pub struct IosRuntime {
    app: App<IosRenderer>,
}

impl IosRuntime {
    pub fn new(callbacks: HostCallbacks) -> mlua::Result<Self> {
        let renderer = IosRenderer::new(callbacks);
        let app = App::new(renderer)?;
        Ok(Self { app })
    }

    pub fn load_lua(&mut self, source: &str) -> mlua::Result<()> {
        self.app.run_script(source)?;
        self.app.mount()
    }

    pub fn tick(&mut self) -> mlua::Result<bool> {
        self.app.tick()
    }

    pub fn next_wake_ms(&self) -> i32 {
        self.app
            .next_wake_time()
            .map(|wake| {
                let now = Instant::now();
                if wake <= now {
                    0
                } else {
                    wake.saturating_duration_since(now)
                        .as_millis()
                        .min(i32::MAX as u128) as i32
                }
            })
            .unwrap_or(-1)
    }

    pub fn dispatch_click(&mut self, id: u32) {
        self.app.push_event(UiEvent::Click {
            node_id: NodeId::from_u32(id),
        });
    }

    pub fn dispatch_input(&mut self, id: u32, value: String) {
        self.app.push_event(UiEvent::Change {
            node_id: NodeId::from_u32(id),
            value,
        });
    }

    pub fn dispatch_submit(&mut self, id: u32, value: String) {
        self.app.push_event(UiEvent::Submit {
            node_id: NodeId::from_u32(id),
            value,
        });
    }

    pub fn dispatch_toggle(&mut self, id: u32, checked: bool) {
        self.app.push_event(UiEvent::Toggle {
            node_id: NodeId::from_u32(id),
            checked,
        });
    }

    pub fn set_viewport(&mut self, width: u16, height: u16) {
        self.app.set_viewport_size(width, height);
        self.app
            .renderer()
            .set_viewport_size(width as f32, height as f32);
        let root = self.app.registry().borrow().root();
        if let Some(root) = root {
            self.app.registry().borrow_mut().mark_dirty(root);
        }
    }
}

#[derive(Debug, Clone)]
pub enum IosDestination {
    Simulator,
    Device { id: Option<String> },
}

#[derive(Debug, Clone)]
pub struct IosRunOptions {
    pub destination: IosDestination,
    pub app_name: String,
    pub bundle_id: String,
    pub team_id: Option<String>,
}

impl Default for IosRunOptions {
    fn default() -> Self {
        Self {
            destination: IosDestination::Simulator,
            app_name: "Rover".to_string(),
            bundle_id: "lu.rover.generated.rover".to_string(),
            team_id: None,
        }
    }
}

pub fn run_file(file: &Path, _args: &[String]) -> Result<()> {
    let source = fs::read_to_string(file)?;
    let mut runtime = IosRuntime::new(HostCallbacks::default())?;
    runtime.load_lua(&source)?;
    runtime.tick()?;
    println!("iOS runtime mounted. UIKit host bridge scaffold ready.");
    Ok(())
}

pub fn launch_file(file: &Path, args: &[String], options: IosRunOptions) -> Result<()> {
    let project = IosProject::prepare(file, &options)?;
    project.build_runtime()?;
    project.write_template()?;
    project.apply_plugins_scaffold()?;
    project.build_and_run(args, &options)
}

struct IosProject {
    root: PathBuf,
    source_root: PathBuf,
    source_file: PathBuf,
    runtime_lib: PathBuf,
    lua_lib: PathBuf,
    build_target_dir: PathBuf,
}

impl IosProject {
    fn prepare(file: &Path, _options: &IosRunOptions) -> Result<Self> {
        let project_root = std::env::current_dir()?;
        let source_root = rover_source_root()?;
        let root = project_root.join(".rover/ios");
        let runtime_lib = project_root.join("target/ios/librover_ios.a");
        let lua_lib = project_root.join("target/ios/liblua5.4.a");
        let build_target_dir = project_root.join("target/ios-build");
        Ok(Self {
            root,
            source_root,
            source_file: file.canonicalize()?,
            runtime_lib,
            lua_lib,
            build_target_dir,
        })
    }

    fn build_runtime(&self) -> Result<()> {
        let Some(parent) = self.runtime_lib.parent() else {
            return Err(anyhow::anyhow!("failed to resolve iOS runtime output dir"));
        };
        fs::create_dir_all(parent)?;

        if let Some(packaged) = packaged_ios_runtime()? {
            fs::copy(&packaged.runtime_lib, &self.runtime_lib).with_context(|| {
                format!(
                    "failed to copy packaged iOS runtime from {} to {}",
                    packaged.runtime_lib.display(),
                    self.runtime_lib.display()
                )
            })?;
            fs::copy(&packaged.lua_lib, &self.lua_lib).with_context(|| {
                format!(
                    "failed to copy packaged iOS Lua runtime from {} to {}",
                    packaged.lua_lib.display(),
                    self.lua_lib.display()
                )
            })?;
            return Ok(());
        }

        if !self.source_root.join("Cargo.toml").exists() {
            return Err(anyhow::anyhow!(
                "iOS runtime not packaged. Expected librover_ios.a and liblua5.4.a next to rover or under runtimes/ios"
            ));
        }

        let target = simulator_target();
        ensure_rust_target(target)?;
        let status = Command::new("cargo")
            .env("IPHONEOS_DEPLOYMENT_TARGET", "15.0")
            .env("MACOSX_DEPLOYMENT_TARGET", "15.0")
            .current_dir(&self.source_root)
            .args([
                "build",
                "-p",
                "rover-ios",
                "--target",
                target,
                "--target-dir",
            ])
            .arg(&self.build_target_dir)
            .status()
            .context("failed to build rover-ios runtime")?;
        if !status.success() {
            return Err(anyhow::anyhow!("failed to build rover-ios runtime"));
        }

        let built = self
            .build_target_dir
            .join(target)
            .join("debug")
            .join("librover_ios.a");
        fs::copy(&built, &self.runtime_lib).with_context(|| {
            format!(
                "failed to copy iOS runtime from {} to {}",
                built.display(),
                self.runtime_lib.display()
            )
        })?;

        let built_lua = find_lua_lib(&self.build_target_dir, target)?;
        fs::copy(&built_lua, &self.lua_lib).with_context(|| {
            format!(
                "failed to copy iOS Lua runtime from {} to {}",
                built_lua.display(),
                self.lua_lib.display()
            )
        })?;
        Ok(())
    }

    fn write_template(&self) -> Result<()> {
        let app_dir = self.root.join("RoverIosHost");
        fs::create_dir_all(&app_dir)?;
        fs::write(app_dir.join("RoverIosHost.swift"), IOS_HOST_SWIFT)?;
        fs::write(app_dir.join("Info.plist"), INFO_PLIST)?;
        fs::copy(&self.source_file, app_dir.join("bundle.lua"))?;
        fs::write(self.root.join("README.md"), GENERATED_README)?;
        Ok(())
    }

    fn apply_plugins_scaffold(&self) -> Result<()> {
        let plugins = PathBuf::from("native/ios/plugins");
        if plugins.exists() {
            println!(
                "native iOS plugins found at {}; apply support scaffolded, diff replay pending",
                plugins.display()
            );
        }
        Ok(())
    }

    fn build_and_run(&self, _args: &[String], options: &IosRunOptions) -> Result<()> {
        write_project_file(&self.root, &self.runtime_lib, &self.lua_lib, options)?;
        let derived_data = self.root.join("DerivedData");
        let destination = match &options.destination {
            IosDestination::Simulator => "generic/platform=iOS Simulator".to_string(),
            IosDestination::Device { id: Some(id) } => format!("id={id}"),
            IosDestination::Device { id: None } => "generic/platform=iOS".to_string(),
        };

        let status = Command::new("xcodebuild")
            .arg("-project")
            .arg(self.root.join("RoverIosHost.xcodeproj"))
            .arg("-scheme")
            .arg("RoverIosHost")
            .arg("-destination")
            .arg(destination)
            .arg("-derivedDataPath")
            .arg(&derived_data)
            .arg("build")
            .status()
            .context("failed to run xcodebuild for iOS host")?;

        if !status.success() {
            return Err(anyhow::anyhow!(
                "iOS host build failed with status {status}"
            ));
        }

        println!("Built iOS host at {}", self.root.display());
        if matches!(options.destination, IosDestination::Simulator) {
            let app = derived_data
                .join("Build/Products/Debug-iphonesimulator")
                .join(format!("{}.app", options.app_name));
            install_and_launch_simulator(&app, &options.bundle_id)?;
        }
        Ok(())
    }
}

struct PackagedIosRuntime {
    runtime_lib: PathBuf,
    lua_lib: PathBuf,
}

fn packaged_ios_runtime() -> Result<Option<PackagedIosRuntime>> {
    if let (Some(runtime), Some(lua)) = (
        std::env::var_os("ROVER_IOS_RUNTIME_LIB"),
        std::env::var_os("ROVER_IOS_LUA_LIB"),
    ) {
        let runtime_lib = PathBuf::from(runtime);
        let lua_lib = PathBuf::from(lua);
        if runtime_lib.exists() && lua_lib.exists() {
            return Ok(Some(PackagedIosRuntime {
                runtime_lib,
                lua_lib,
            }));
        }
    }

    let exe = std::env::current_exe()?;
    let Some(dir) = exe.parent() else {
        return Ok(None);
    };
    let dirs = [
        dir.to_path_buf(),
        dir.join("runtimes/ios"),
        dir.join("../share/rover/runtimes/ios"),
    ];

    for dir in dirs {
        let runtime_lib = dir.join("librover_ios.a");
        let lua_lib = dir.join("liblua5.4.a");
        if runtime_lib.exists() && lua_lib.exists() {
            return Ok(Some(PackagedIosRuntime {
                runtime_lib,
                lua_lib,
            }));
        }
    }

    Ok(None)
}

fn simulator_target() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    return "aarch64-apple-ios-sim";
    #[cfg(target_arch = "x86_64")]
    return "x86_64-apple-ios";
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    "aarch64-apple-ios-sim"
}

fn rover_source_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("ROVER_SOURCE_ROOT") {
        return Ok(PathBuf::from(root));
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("failed to resolve Rover source root"))
}

fn ensure_rust_target(target: &str) -> Result<()> {
    let status = Command::new("rustup")
        .args(["target", "add", target])
        .status()
        .context("failed to run rustup target add")?;
    if !status.success() {
        return Err(anyhow::anyhow!("failed to install Rust target {target}"));
    }
    Ok(())
}

fn install_and_launch_simulator(app: &Path, bundle_id: &str) -> Result<()> {
    if !app.exists() {
        return Err(anyhow::anyhow!(
            "built iOS app not found at {}",
            app.display()
        ));
    }

    Command::new("open")
        .args(["-a", "Simulator"])
        .status()
        .context("failed to open Simulator")?;

    ensure_simulator_booted()?;
    let app_path = app.display().to_string();
    run_simctl(["install", "booted", app_path.as_str()], "install iOS app")?;
    run_simctl(["launch", "booted", bundle_id], "launch iOS app")?;

    println!("Launched iOS app {bundle_id}");
    Ok(())
}

fn ensure_simulator_booted() -> Result<()> {
    if Command::new("xcrun")
        .args(["simctl", "bootstatus", "booted", "-b"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to check booted Simulator")?
        .success()
    {
        return Ok(());
    }

    let simulator = select_available_simulator()?;
    let boot_status = Command::new("xcrun")
        .args(["simctl", "boot", &simulator])
        .status()
        .context("failed to boot Simulator")?;
    if !boot_status.success() {
        return Err(anyhow::anyhow!("failed to boot Simulator {simulator}"));
    }

    run_simctl(["bootstatus", &simulator, "-b"], "wait for Simulator boot")
}

fn select_available_simulator() -> Result<String> {
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "available"])
        .output()
        .context("failed to list available Simulators")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("failed to list available Simulators"));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut fallback = None;
    for line in text.lines() {
        if !line.contains("(Shutdown)") {
            continue;
        }
        let Some(id) = simulator_id_from_line(line) else {
            continue;
        };
        if line.contains("iPhone") {
            return Ok(id);
        }
        fallback.get_or_insert(id);
    }

    fallback.ok_or_else(|| anyhow::anyhow!("no available shutdown Simulator found"))
}

fn simulator_id_from_line(line: &str) -> Option<String> {
    let end = line.rfind(") (Shutdown)")?;
    let before_status = &line[..end];
    let start = before_status.rfind('(')?;
    Some(before_status[start + 1..].to_string())
}

fn run_simctl<const N: usize>(args: [&str; N], action: &str) -> Result<()> {
    let status = Command::new("xcrun")
        .arg("simctl")
        .args(args)
        .status()
        .with_context(|| format!("failed to {action}"))?;
    if !status.success() {
        return Err(anyhow::anyhow!("failed to {action}"));
    }
    Ok(())
}

fn find_lua_lib(target_dir: &Path, target: &str) -> Result<PathBuf> {
    let build_dir = target_dir.join(target).join("debug/build");
    for entry in fs::read_dir(&build_dir)
        .with_context(|| format!("failed to read {}", build_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path().join("out/lib/liblua5.4.a");
        if path.exists() {
            return Ok(path);
        }
    }

    Err(anyhow::anyhow!(
        "failed to find vendored Lua archive under {}",
        build_dir.display()
    ))
}

fn write_project_file(
    root: &Path,
    runtime_lib: &Path,
    lua_lib: &Path,
    options: &IosRunOptions,
) -> Result<()> {
    let project_dir = root.join("RoverIosHost.xcodeproj");
    fs::create_dir_all(&project_dir)?;
    let team = pbx_string(options.team_id.as_deref().unwrap_or(""));
    let app_name = pbx_string(&options.app_name);
    let app_bundle_name = pbx_string(&format!("{}.app", options.app_name));
    let bundle_id = pbx_string(&options.bundle_id);
    let runtime_lib_path = pbx_string(&runtime_lib.display().to_string());
    let lua_lib_path = pbx_string(&lua_lib.display().to_string());
    let runtime_lib_dir_path = pbx_string(
        &runtime_lib
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .display()
            .to_string(),
    );
    let pbxproj = PROJECT_PBXPROJ
        .replace("__BUNDLE_ID__", &bundle_id)
        .replace("__TEAM_ID__", &team)
        .replace("__APP_NAME__", &app_name)
        .replace("__APP_BUNDLE_NAME__", &app_bundle_name)
        .replace("__RUNTIME_LIB__", &runtime_lib_path)
        .replace("__LUA_LIB__", &lua_lib_path)
        .replace("__RUNTIME_LIB_DIR__", &runtime_lib_dir_path)
        .replace("__SIM_ARCH__", simulator_arch());
    fs::write(project_dir.join("project.pbxproj"), pbxproj)?;
    Ok(())
}

fn pbx_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn simulator_arch() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    return "arm64";
    #[cfg(target_arch = "x86_64")]
    return "x86_64";
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    "arm64"
}

const GENERATED_README: &str = r#"# Generated Rover iOS Project

This directory is managed by Rover.

Edit it when exploring native changes, then capture managed native plugins with:

```bash
rover capture -p ios <name>
```
"#;

const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>$(DEVELOPMENT_LANGUAGE)</string>
  <key>CFBundleExecutable</key>
  <string>$(EXECUTABLE_NAME)</string>
  <key>CFBundleIdentifier</key>
  <string>$(PRODUCT_BUNDLE_IDENTIFIER)</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$(PRODUCT_NAME)</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>UILaunchScreen</key>
  <dict/>
  <key>UIApplicationSceneManifest</key>
  <dict>
    <key>UIApplicationSupportsMultipleScenes</key>
    <false/>
  </dict>
</dict>
</plist>
"#;

const IOS_HOST_SWIFT: &str = include_str!("../template/RoverIosHost.swift");
const PROJECT_PBXPROJ: &str = include_str!("../template/project.pbxproj");
