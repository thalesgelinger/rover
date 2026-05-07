use anyhow::{Context, Result};
use colored::Colorize;
use rover_bundler::{BundleOptions, bundle, serialize_bundle};
use std::fs;
use std::path::PathBuf;

/// Supported build targets (Deno-compatible)
pub const SUPPORTED_TARGETS: &[&str] = &[
    "macos",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
];

/// Build options
pub struct BuildOptions {
    pub entrypoint: PathBuf,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
}

/// Run the build process
pub fn run_build(options: BuildOptions) -> Result<()> {
    println!("{}", "📦 Building Rover application...".cyan());

    // Validate target if provided
    let target = options.target.unwrap_or_else(|| get_host_target());
    if !SUPPORTED_TARGETS.contains(&target.as_str()) {
        return Err(anyhow::anyhow!(
            "Unsupported target: {}\n\nSupported targets:\n{}\n\nUse --target to specify a supported target.",
            target,
            SUPPORTED_TARGETS
                .iter()
                .map(|t| format!("  - {}", t))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    // Step 1: Bundle the application
    let base_path = options
        .entrypoint
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    let bundle_options = BundleOptions {
        entrypoint: options.entrypoint.clone(),
        base_path,
    };

    let bundle = bundle(bundle_options).context("Failed to bundle application")?;

    println!("  {} Bundled {} files", "✓".green(), bundle.files.len());

    // Step 2: Detect features
    let features = &bundle.features;
    println!("  {} Detected features:", "✓".green());
    if features.server {
        println!("     - Server");
    }
    if features.ui {
        println!("     - UI");
    }
    if !features.server && !features.ui {
        println!("     - Script (no server/ui)");
    }

    // Step 3: Serialize bundle
    let bundle_lua = serialize_bundle(&bundle);

    if target == "macos" {
        let output_name = options.output.unwrap_or_else(|| {
            let stem = options
                .entrypoint
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            PathBuf::from(format!("{}.app", stem))
        });
        create_macos_app(&bundle_lua, &output_name)?;
        println!("{}", format!("✅ Built: {}", output_name.display()).green());
        return Ok(());
    }

    // Step 4: Select runtime
    let runtime_path = find_runtime(&target, features)?;

    println!(
        "  {} Using runtime: {}",
        "✓".green(),
        runtime_path.display()
    );

    // Step 5: Create output binary
    let output_name = options.output.unwrap_or_else(|| {
        let stem = options
            .entrypoint
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        PathBuf::from(stem.to_string())
    });

    // Step 6: Embed bundle into runtime
    create_binary(&runtime_path, &bundle_lua, &output_name)?;

    println!("{}", format!("✅ Built: {}", output_name.display()).green());
    println!("   Run with: ./{} <args>", output_name.display());

    Ok(())
}

fn create_macos_app(bundle: &str, output: &PathBuf) -> Result<()> {
    let contents = output.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");

    if output.exists() {
        fs::remove_dir_all(output).context("Failed to replace existing .app")?;
    }

    fs::create_dir_all(&macos).context("Failed to create .app MacOS directory")?;
    fs::create_dir_all(&resources).context("Failed to create .app Resources directory")?;

    let name = output
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>{name}</string>
  <key>CFBundleIdentifier</key><string>dev.rover.{name}</string>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
</dict>
</plist>
"#
    );
    fs::write(contents.join("Info.plist"), plist).context("Failed to write Info.plist")?;
    fs::write(resources.join("bundle.lua"), bundle).context("Failed to write macOS bundle")?;

    let host = rover_macos::build_host().context("Failed to build macOS host")?;
    let dylib = host
        .parent()
        .unwrap_or(std::path::Path::new("target/debug"))
        .join("librover_macos.dylib");
    if !dylib.exists() {
        return Err(anyhow::anyhow!(
            "missing macOS runtime library: {}",
            dylib.display()
        ));
    }

    let launcher = macos.join(&name);
    fs::copy(&host, &launcher).context("Failed to copy macOS host")?;
    fs::copy(&dylib, macos.join("librover_macos.dylib"))
        .context("Failed to copy macOS runtime library")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&launcher)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&launcher, perms)?;
    }

    Ok(())
}

/// Get the host target triple
fn get_host_target() -> String {
    // Use rustc's host target
    std::env::var("TARGET").unwrap_or_else(|_| {
        // Fallback to common targets based on OS
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        return "aarch64-apple-darwin".to_string();
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        return "x86_64-apple-darwin".to_string();
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        return "aarch64-unknown-linux-gnu".to_string();
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        return "x86_64-unknown-linux-gnu".to_string();
        #[cfg(target_os = "windows")]
        return "x86_64-pc-windows-msvc".to_string();
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
            target_os = "windows"
        )))]
        "x86_64-unknown-linux-gnu".to_string()
    })
}

/// Find the appropriate runtime binary
fn find_runtime(_target: &str, _features: &rover_parser::AppFeatures) -> Result<PathBuf> {
    // For now, use the local runtime binary
    // In the future, this will select feature-specific and target-specific runtimes
    let search_paths = [
        PathBuf::from("./target/release/rover-runtime"),
        PathBuf::from("./target/debug/rover-runtime"),
    ];

    for path in &search_paths {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    // Error with helpful message
    Err(anyhow::anyhow!(
        "Runtime binary not found\n\nExpected at one of:\n{}\n\nTo build locally:\n  cargo build --package rover_runtime --release\n\nFor cross-compilation, prebuilt runtimes will be downloaded in the future.",
        search_paths
            .iter()
            .map(|p| format!("  - {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

/// Create the final binary by embedding bundle into runtime
fn create_binary(runtime_path: &PathBuf, bundle: &str, output: &PathBuf) -> Result<()> {
    // Read runtime binary
    let mut runtime = fs::read(runtime_path).context("Failed to read runtime binary")?;

    // Convert bundle to bytes
    let bundle_bytes = bundle.as_bytes();

    // Calculate offset (end of runtime)
    let offset = runtime.len();
    let length = bundle_bytes.len();

    // Append bundle
    runtime.extend_from_slice(bundle_bytes);

    // Append trailer: "ROVER\n<offset>\n<length>\n"
    let trailer = format!("ROVER\n{}\n{}\n", offset, length);
    runtime.extend_from_slice(trailer.as_bytes());

    // Write output binary
    fs::write(output, &runtime).context("Failed to write output binary")?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(output)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(output, perms)?;
    }

    Ok(())
}
