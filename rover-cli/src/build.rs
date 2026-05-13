use anyhow::{Context, Result};
use colored::Colorize;
use rover_bundler::{BundleOptions, bundle, serialize_bundle};
use std::fs;
use std::path::{Path, PathBuf};

/// Supported build targets (Deno-compatible)
pub const SUPPORTED_TARGETS: &[&str] = &[
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
    if output_name.is_absolute() {
        println!("   Run with: {} <args>", output_name.display());
    } else {
        println!("   Run with: ./{} <args>", output_name.display());
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
fn find_runtime(target: &str, _features: &rover_parser::AppFeatures) -> Result<PathBuf> {
    let host = get_host_target();
    if target != host {
        return Err(anyhow::anyhow!(
            "Cross-target builds need a packaged Rover runtime for {target}; this rover has {host}"
        ));
    }

    std::env::current_exe().context("Failed to resolve installed rover runtime")
}

/// Create the final binary by embedding bundle into runtime
fn create_binary(runtime_path: &Path, bundle: &str, output: &Path) -> Result<()> {
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
