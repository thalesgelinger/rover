use crate::renderer::AndroidRenderer;
use anyhow::{Context, Result};
use jni::JavaVM;
use jni::objects::GlobalRef;
use rover_ui::app::App;
use rover_ui::events::UiEvent;
use rover_ui::ui::NodeId;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

pub struct AndroidRuntime {
    app: App<AndroidRenderer>,
}

impl AndroidRuntime {
    pub fn new(vm: JavaVM, host: GlobalRef) -> mlua::Result<Self> {
        let renderer = AndroidRenderer::new(vm, host);
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
pub enum AndroidDestination {
    Default,
    Device { id: Option<String> },
}

#[derive(Debug, Clone)]
pub struct AndroidRunOptions {
    pub destination: AndroidDestination,
    pub app_name: String,
    pub package_name: String,
}

impl Default for AndroidRunOptions {
    fn default() -> Self {
        Self {
            destination: AndroidDestination::Default,
            app_name: "Rover".to_string(),
            package_name: "lu.rover.generated.rover".to_string(),
        }
    }
}

pub fn run_file(file: &Path, _args: &[String]) -> Result<()> {
    let source = fs::read_to_string(file)?;
    println!(
        "Android runtime loaded {} bytes. Android host bridge runs on device.",
        source.len()
    );
    Ok(())
}

pub fn launch_file(file: &Path, args: &[String], options: AndroidRunOptions) -> Result<()> {
    let project = AndroidProject::prepare(file, &options)?;
    project.build_runtime()?;
    project.write_template(&options)?;
    project.apply_plugins_scaffold()?;
    project.build_and_run(args, &options)
}

struct AndroidProject {
    root: PathBuf,
    source_root: PathBuf,
    source_file: PathBuf,
    runtime_lib: PathBuf,
    build_target_dir: PathBuf,
}

impl AndroidProject {
    fn prepare(file: &Path, _options: &AndroidRunOptions) -> Result<Self> {
        let project_root = std::env::current_dir()?;
        let source_root = rover_source_root()?;
        let root = project_root.join(".rover/android");
        let runtime_lib = project_root.join("target/android/librover_android.so");
        let build_target_dir = project_root.join("target/android-build");
        Ok(Self {
            root,
            source_root,
            source_file: file.canonicalize()?,
            runtime_lib,
            build_target_dir,
        })
    }

    fn build_runtime(&self) -> Result<()> {
        ensure_android_sdk()?;
        let Some(parent) = self.runtime_lib.parent() else {
            return Err(anyhow::anyhow!(
                "failed to resolve Android runtime output dir"
            ));
        };
        fs::create_dir_all(parent)?;

        let target = android_target();
        ensure_rust_target(target)?;
        let linker = android_linker()?;
        let status = Command::new("cargo")
            .env("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", linker)
            .env("CC_aarch64_linux_android", android_linker()?)
            .env(
                "CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS",
                "-C link-arg=-Wl,-z,max-page-size=16384",
            )
            .current_dir(&self.source_root)
            .args([
                "build",
                "-p",
                "rover-android",
                "--target",
                target,
                "--target-dir",
            ])
            .arg(&self.build_target_dir)
            .status()
            .context("failed to build rover-android runtime")?;
        if !status.success() {
            return Err(anyhow::anyhow!("failed to build rover-android runtime"));
        }

        let built = self
            .build_target_dir
            .join(target)
            .join("debug")
            .join("librover_android.so");
        fs::copy(&built, &self.runtime_lib).with_context(|| {
            format!(
                "failed to copy Android runtime from {} to {}",
                built.display(),
                self.runtime_lib.display()
            )
        })?;
        Ok(())
    }

    fn write_template(&self, options: &AndroidRunOptions) -> Result<()> {
        let app_dir = self.root.join("app/src/main");
        let kotlin_dir = app_dir.join("java/lu/rover/host");
        let jni_dir = app_dir.join("jniLibs/arm64-v8a");
        let assets_dir = app_dir.join("assets");
        fs::create_dir_all(&kotlin_dir)?;
        fs::create_dir_all(&jni_dir)?;
        fs::create_dir_all(&assets_dir)?;
        fs::write(self.root.join("settings.gradle.kts"), SETTINGS_GRADLE)?;
        fs::write(self.root.join("build.gradle.kts"), ROOT_BUILD_GRADLE)?;
        fs::write(self.root.join("gradle.properties"), GRADLE_PROPERTIES)?;
        fs::write(
            self.root.join("app/build.gradle.kts"),
            APP_BUILD_GRADLE
                .replace("__PACKAGE_NAME__", &options.package_name)
                .replace("__APP_NAME__", &options.app_name),
        )?;
        fs::write(
            app_dir.join("AndroidManifest.xml"),
            ANDROID_MANIFEST.replace("__APP_NAME__", &options.app_name),
        )?;
        fs::write(kotlin_dir.join("MainActivity.kt"), MAIN_ACTIVITY_KT)?;
        fs::write(kotlin_dir.join("RoverRuntime.kt"), ROVER_RUNTIME_KT)?;
        fs::copy(&self.source_file, assets_dir.join("bundle.lua"))?;
        fs::copy(&self.runtime_lib, jni_dir.join("librover_android.so"))?;
        fs::write(self.root.join("README.md"), GENERATED_README)?;
        Ok(())
    }

    fn apply_plugins_scaffold(&self) -> Result<()> {
        let plugins = PathBuf::from("native/android/plugins");
        if plugins.exists() {
            println!(
                "native Android plugins found at {}; apply support scaffolded, diff replay pending",
                plugins.display()
            );
        }
        Ok(())
    }

    fn build_and_run(&self, _args: &[String], options: &AndroidRunOptions) -> Result<()> {
        let gradle = gradle_command(&self.root);
        let status = Command::new(&gradle)
            .arg(":app:assembleDebug")
            .current_dir(&self.root)
            .status()
            .with_context(|| format!("failed to run {gradle} for Android host"))?;
        if !status.success() {
            return Err(anyhow::anyhow!(
                "Android host build failed with status {status}"
            ));
        }

        let apk = self.root.join("app/build/outputs/apk/debug/app-debug.apk");
        let mut adb = adb_command(&options.destination);
        adb.arg("install").arg("-r").arg(&apk);
        run_command(adb, "install Android app")?;

        let mut adb = adb_command(&options.destination);
        adb.arg("shell")
            .arg("am")
            .arg("start")
            .arg("-n")
            .arg(format!(
                "{}/lu.rover.host.MainActivity",
                options.package_name
            ));
        run_command(adb, "launch Android app")?;

        println!("Launched Android app {}", options.package_name);
        Ok(())
    }
}

fn android_target() -> &'static str {
    "aarch64-linux-android"
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

fn ensure_android_sdk() -> Result<()> {
    if std::env::var_os("ANDROID_HOME").is_none() && std::env::var_os("ANDROID_SDK_ROOT").is_none()
    {
        return Err(anyhow::anyhow!(
            "Android run needs ANDROID_HOME or ANDROID_SDK_ROOT"
        ));
    }
    Ok(())
}

fn android_linker() -> Result<PathBuf> {
    if let Some(ndk) = std::env::var_os("ANDROID_NDK_HOME") {
        return Ok(ndk_toolchain(PathBuf::from(ndk)));
    }

    let sdk = std::env::var_os("ANDROID_HOME")
        .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
        .ok_or_else(|| anyhow::anyhow!("Android run needs ANDROID_HOME or ANDROID_SDK_ROOT"))?;
    let ndk_dir = PathBuf::from(sdk).join("ndk");
    let mut versions = fs::read_dir(&ndk_dir)
        .with_context(|| format!("failed to read Android NDK dir {}", ndk_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    versions.sort();
    let ndk = versions
        .pop()
        .ok_or_else(|| anyhow::anyhow!("no Android NDK found under {}", ndk_dir.display()))?;
    Ok(ndk_toolchain(ndk))
}

fn ndk_toolchain(ndk: PathBuf) -> PathBuf {
    let host = if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    };
    ndk.join("toolchains/llvm/prebuilt")
        .join(host)
        .join("bin/aarch64-linux-android23-clang")
}

fn gradle_command(root: &Path) -> String {
    let wrapper = root.join("gradlew");
    if wrapper.exists() {
        wrapper.display().to_string()
    } else {
        "gradle".to_string()
    }
}

fn adb_command(destination: &AndroidDestination) -> Command {
    let mut command = Command::new("adb");
    if let AndroidDestination::Device { id: Some(id) } = destination {
        command.arg("-s").arg(id);
    }
    command
}

fn run_command(mut command: Command, action: &str) -> Result<()> {
    let status = command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to {action}"))?;
    if !status.success() {
        return Err(anyhow::anyhow!("failed to {action}"));
    }
    Ok(())
}

const GENERATED_README: &str = r#"# Generated Rover Android Project

This directory is managed by Rover.

Edit it when exploring native changes, then capture managed native plugins with:

```bash
rover capture -p android <name>
```
"#;

const SETTINGS_GRADLE: &str = include_str!("../template/settings.gradle.kts");
const ROOT_BUILD_GRADLE: &str = include_str!("../template/build.gradle.kts");
const GRADLE_PROPERTIES: &str = include_str!("../template/gradle.properties");
const APP_BUILD_GRADLE: &str = include_str!("../template/app.build.gradle.kts");
const ANDROID_MANIFEST: &str = include_str!("../template/AndroidManifest.xml");
const MAIN_ACTIVITY_KT: &str = include_str!("../template/MainActivity.kt");
const ROVER_RUNTIME_KT: &str = include_str!("../template/RoverRuntime.kt");
