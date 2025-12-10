use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};

const BUILD_ROOT: &str = ".rover/build/android";
const PACKAGE_NAME: &str = "dev.rover.app";
const ANDROID_TARGET: &str = "aarch64-linux-android";
const MIN_API: u32 = 28;
const GRADLE_VERSION: &str = "8.2.1";
const DEV_CONFIG_NAME: &str = "rover.lua";

pub struct AndroidRunner {
    build_dir: PathBuf,
}

impl AndroidRunner {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            build_dir: cwd.join(BUILD_ROOT),
        }
    }

    pub fn ensure_prereqs(&self) -> Result<()> {
        check_cmd("adb")?;
        
        // Check ANDROID_HOME or ANDROID_SDK_ROOT
        let sdk_root = std::env::var("ANDROID_HOME")
            .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
            .context("ANDROID_HOME or ANDROID_SDK_ROOT not set")?;
        
        // Check NDK
        let _ndk_root = std::env::var("ANDROID_NDK_ROOT")
            .or_else(|_| detect_ndk_from_sdk(&sdk_root))?;
        
        // Verify rustup target installed
        let output = Command::new("rustup")
            .args(["target", "list", "--installed"])
            .output()
            .context("rustup target list")?;
        let installed = String::from_utf8_lossy(&output.stdout);
        if !installed.contains(ANDROID_TARGET) {
            return Err(anyhow!(
                "Android target not installed. Run: rustup target add {}",
                ANDROID_TARGET
            ));
        }

        Ok(())
    }

    pub fn stage_payload(&self, entry: &Path) -> Result<()> {
        let entry = entry
            .canonicalize()
            .with_context(|| format!("canonicalize {}", entry.display()))?;
        let assets_src = entry.parent().map(|p| p.join("assets"));

        let project_assets = self.build_dir.join("project/app/src/main/assets/rover");
        fs::create_dir_all(&project_assets).context("create assets dir")?;

        fs::copy(&entry, project_assets.join("main.lua")).context("copy main.lua")?;

        if let Some(assets) = assets_src {
            if assets.exists() {
                let dest = project_assets.join("assets");
                if dest.exists() {
                    fs::remove_dir_all(&dest).context("clean old assets")?;
                }
                fs::create_dir_all(&dest).context("create assets dest")?;
                copy_dir(&assets, &dest)?;
            }
        }

        // Copy dev config if present
        let cfg = entry.parent().map(|p| p.join(DEV_CONFIG_NAME));
        if let Some(cfg_path) = cfg {
            if cfg_path.exists() {
                fs::copy(&cfg_path, project_assets.join(DEV_CONFIG_NAME))
                    .with_context(|| format!("copy {}", cfg_path.display()))?;
            }
        }

        Ok(())
    }

    pub fn generate_project(&self) -> Result<PathBuf> {
        fs::create_dir_all(&self.build_dir).context("create build dir")?;
        let template = Path::new("platform/android-runner/templates/android-empty");
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
        
        Ok(out)
    }

    pub fn build_rust_shared(&self) -> Result<PathBuf> {
        let ndk_root = std::env::var("ANDROID_NDK_ROOT")
            .or_else(|_| {
                let sdk = std::env::var("ANDROID_HOME")
                    .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))?;
                detect_ndk_from_sdk(&sdk)
            })?;

        let host = detect_host_tag()?;
        let toolchain_base = format!("{}/toolchains/llvm/prebuilt/{}", ndk_root, host);

        let cc = format!("{}/bin/aarch64-linux-android{}-clang", toolchain_base, MIN_API);
        let ar = format!("{}/bin/llvm-ar", toolchain_base);

        let status = Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg("rover-runtime")
            .arg("--target")
            .arg(ANDROID_TARGET)
            .env(format!("CC_{}", ANDROID_TARGET.replace('-', "_")), &cc)
            .env(format!("AR_{}", ANDROID_TARGET.replace('-', "_")), &ar)
            .status()
            .context("cargo build rover-runtime (android)")?;

        if !status.success() {
            return Err(anyhow!("cargo build rover-runtime failed"));
        }

        let lib = PathBuf::from("target")
            .join(ANDROID_TARGET)
            .join("debug/librover_runtime.so");

        if !lib.exists() {
            return Err(anyhow!("shared lib missing at {}", lib.display()));
        }

        Ok(lib)
    }

    pub fn build_apk(&self, shared: &Path) -> Result<PathBuf> {
        let project = self.build_dir.join("project");

        let jni_libs = project.join("app/src/main/jniLibs/arm64-v8a");
        fs::create_dir_all(&jni_libs).context("create jniLibs dir")?;
        fs::copy(shared, jni_libs.join("librover_runtime.so"))
            .context("copy shared lib")?;

        let wrapper = ensure_gradle_wrapper(&project)?;

        let status = Command::new(&wrapper)
            .current_dir(&project)
            .arg("assembleDebug")
            .status()
            .with_context(|| format!("{} assembleDebug", wrapper.display()))?;

        if !status.success() {
            return Err(anyhow!("gradle assembleDebug failed"));
        }

        let apk = project.join("app/build/outputs/apk/debug/app-debug.apk");
        if !apk.exists() {
            return Err(anyhow!("APK not found at {}", apk.display()));
        }

        Ok(apk)
    }

    pub fn install_and_launch(&self, apk: &Path) -> Result<()> {
        let status = Command::new("adb")
            .args(["install", "-r"])
            .arg(apk)
            .status()
            .context("adb install")?;

        if !status.success() {
            return Err(anyhow!("adb install failed"));
        }

        let activity = format!("{}/.MainActivity", PACKAGE_NAME);
        let status = Command::new("adb")
            .args(["shell", "am", "start", "-n", &activity])
            .status()
            .context("adb launch")?;

        if !status.success() {
            return Err(anyhow!("adb launch failed"));
        }

        Ok(())
    }

    pub fn build_only(&self, entry: &Path) -> Result<PathBuf> {
        self.ensure_prereqs()?;
        if self.build_dir.exists() {
            fs::remove_dir_all(&self.build_dir).ok();
        }

        let _project = self.generate_project()?;
        self.stage_payload(entry)?;
        println!("[rover][android] building rust shared...");
        let lib = self.build_rust_shared()?;
        println!("[rover][android] building apk...");
        self.build_apk(&lib)
    }

    pub fn build_and_run(&self, entry: &Path) -> Result<()> {
        self.ensure_prereqs()?;
        if self.build_dir.exists() {
            fs::remove_dir_all(&self.build_dir).ok();
        }

        let _project = self.generate_project()?;
        self.stage_payload(entry)?;
        println!("[rover][android] building rust shared...");
        let lib = self.build_rust_shared()?;
        println!("[rover][android] building apk...");
        let apk = self.build_apk(&lib)?;
        println!("[rover][android] installing and launching...");
        self.install_and_launch(&apk)?;

        Ok(())
    }
}

fn check_cmd(cmd: &str) -> Result<()> {
    let which_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    
    let status = Command::new(which_cmd)
        .arg(cmd)
        .status()
        .with_context(|| format!("{} {}", which_cmd, cmd))?;
    
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{} not found", cmd))
    }
}

fn find_gradle_cmd(project: &Path) -> Result<PathBuf> {
    let wrapper = project.join(if cfg!(target_os = "windows") { "gradlew.bat" } else { "gradlew" });
    if wrapper.exists() {
        return Ok(wrapper);
    }

    if let Ok(home) = std::env::var("GRADLE_HOME") {
        let candidate = PathBuf::from(home).join("bin").join(if cfg!(target_os = "windows") { "gradle.bat" } else { "gradle" });
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let studio_gradle_root = Path::new("/Applications/Android Studio.app/Contents/gradle");
        if studio_gradle_root.exists() {
            if let Some(cmd) = latest_gradle_in(studio_gradle_root) {
                return Ok(cmd);
            }
        }
    }

    if let Some(cmd) = which_path("gradle") {
        return Ok(cmd);
    }

    Err(anyhow!("gradle not found. Install Gradle, set GRADLE_HOME, or include gradlew in the template."))
}

fn ensure_gradle_wrapper(project: &Path) -> Result<PathBuf> {
    let wrapper = project.join(if cfg!(target_os = "windows") { "gradlew.bat" } else { "gradlew" });
    if wrapper.exists() {
        return Ok(wrapper);
    }

    let gradle = find_gradle_cmd(project)?;
    if !gradle_version_supported(&gradle)? {
        return Err(anyhow!(
            "Gradle {:?} unsupported; use Gradle 8.0-8.2.x or provide gradlew",
            gradle
        ));
    }

    let status = Command::new(&gradle)
        .current_dir(project)
        .args(["wrapper", "--gradle-version", GRADLE_VERSION, "--distribution-type", "all"])
        .status()
        .with_context(|| format!("{} wrapper --gradle-version {}", gradle.display(), GRADLE_VERSION))?;
    if !status.success() {
        return Err(anyhow!("gradle wrapper generation failed"));
    }

    if wrapper.exists() {
        Ok(wrapper)
    } else {
        Err(anyhow!("gradle wrapper not created"))
    }
}

fn gradle_version_supported(gradle: &Path) -> Result<bool> {
    let output = Command::new(gradle)
        .arg("--version")
        .output()
        .with_context(|| format!("{} --version", gradle.display()))?;
    if !output.status.success() {
        return Ok(false);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Gradle ") {
            let parts: Vec<_> = rest.split('.').collect();
            if parts.len() >= 2 {
                let major: u32 = parts[0].parse().unwrap_or(0);
                let minor: u32 = parts[1].parse().unwrap_or(0);
                return Ok(major == 8 && minor <= 2);
            }
        }
    }
    Ok(false)
}

fn which_path(cmd: &str) -> Option<PathBuf> {


    let which_cmd = if cfg!(target_os = "windows") { "where" } else { "which" };
    if let Ok(output) = Command::new(which_cmd).arg(cmd).output() {
        if output.status.success() {
            if let Some(line) = String::from_utf8_lossy(&output.stdout).lines().next() {
                return Some(PathBuf::from(line.trim()));
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn latest_gradle_in(root: &Path) -> Option<PathBuf> {
    let mut versions: Vec<PathBuf> = fs::read_dir(root).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    versions.sort();
    versions.pop().map(|p| p.join("bin").join("gradle"))
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

fn detect_ndk_from_sdk(sdk_root: &str) -> Result<String> {
    let ndk_dir = PathBuf::from(sdk_root).join("ndk");
    if !ndk_dir.exists() {
        return Err(anyhow!("NDK not found in SDK. Install via sdkmanager."));
    }
    
    // Find highest version
    let mut versions: Vec<String> = fs::read_dir(&ndk_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().ok().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    
    versions.sort();
    versions.last()
        .map(|v| ndk_dir.join(v).to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("No NDK version found in {}", ndk_dir.display()))
}

fn detect_host_tag() -> Result<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    
    let tag = match (os, arch) {
        ("macos", "x86_64") => "darwin-x86_64",
        ("macos", "aarch64") => "darwin-x86_64", // NDK uses x86_64 for both
        ("linux", "x86_64") => "linux-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => return Err(anyhow!("unsupported host: {}-{}", os, arch)),
    };
    
    Ok(tag.to_string())
}
