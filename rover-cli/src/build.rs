use anyhow::{Context, Result};
use colored::Colorize;
use rover_bundler::{BundleOptions, bundle, serialize_bundle};
use std::fs;
use std::path::PathBuf;

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
    println!("   Run with: ./{} <args>", output_name.display());

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
fn find_runtime(target: &str, features: &rover_parser::AppFeatures) -> Result<PathBuf> {
    // Build runtime feature suffix
    let mut feature_suffix = String::new();
    if features.server {
        feature_suffix.push_str("-server");
    }
    if features.ui {
        feature_suffix.push_str("-ui");
    }

    // For now, use a generic runtime (will be replaced with feature-specific ones)
    let runtime_name = format!("rover-runtime-{}{}", target, feature_suffix);

    // Search paths (in order of priority)
    let search_paths = [
        PathBuf::from(format!("./target/{}", runtime_name)),
        PathBuf::from(format!("./target/release/{}-runtime", runtime_name)),
        PathBuf::from(format!("./target/debug/{}-runtime", runtime_name)),
    ];

    for path in &search_paths {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    // If no feature-specific runtime found, try generic
    let generic_runtime = format!("rover-runtime-{}", target);
    let generic_paths = [
        PathBuf::from(format!("./target/{}", generic_runtime)),
        PathBuf::from(format!("./target/release/{}-runtime", generic_runtime)),
        PathBuf::from(format!("./target/debug/{}-runtime", generic_runtime)),
    ];

    for path in &generic_paths {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    // Error with helpful message
    Err(anyhow::anyhow!(
        "Runtime not found for target: {}\n\nExpected at one of:\n{}\n\nTo build locally:\n  cargo build --package rover-runtime --release\n\nFor cross-compilation, prebuilt runtimes will be downloaded in the future.",
        target,
        search_paths
            .iter()
            .chain(generic_paths.iter())
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
