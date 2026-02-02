use anyhow::Result;
use std::env;

fn main() -> Result<()> {
    // Get command line arguments
    let args: Vec<String> = env::args().collect();

    // Load embedded bundle
    let bundle = match load_embedded_bundle() {
        Some(bundle) => bundle,
        None => {
            eprintln!("Error: No embedded bundle found");
            eprintln!("This binary appears to be an unbundled runtime.");
            std::process::exit(1);
        }
    };

    // Run the bundled application using rover_core
    rover_core::run_from_str(&bundle, &args[1..], false)
}

/// Load embedded bundle from binary trailer
fn load_embedded_bundle() -> Option<String> {
    // Read self binary
    let exe_path = env::current_exe().ok()?;
    let data = std::fs::read(&exe_path).ok()?;

    // Look for trailer: "ROVER\n<offset>\n<length>\n"
    const TRAILER_MAGIC: &[u8] = b"ROVER\n";

    if let Some(pos) = data
        .windows(TRAILER_MAGIC.len())
        .rposition(|w| w == TRAILER_MAGIC)
    {
        let trailer_start = pos;
        let trailer = &data[trailer_start..];

        // Parse offset and length
        let trailer_str = std::str::from_utf8(trailer).ok()?;
        let parts: Vec<&str> = trailer_str.split('\n').collect();

        if parts.len() >= 3 {
            let offset: usize = parts[1].parse().ok()?;
            let length: usize = parts[2].parse().ok()?;

            if offset + length <= data.len() {
                let bundle = &data[offset..offset + length];
                return String::from_utf8(bundle.to_vec()).ok();
            }
        }
    }

    None
}
