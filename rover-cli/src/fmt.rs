use anyhow::{Context, Result};
use rover_parser::format_code;
use std::path::PathBuf;

pub struct FmtOptions {
    pub file: Option<PathBuf>,
    pub check: bool,
}

pub fn run_fmt(opts: FmtOptions) -> Result<()> {
    let files = match opts.file {
        Some(path) => vec![path],
        None => find_lua_files(".")?,
    };

    let mut needs_format = false;

    for file in files {
        let result = format_file(&file, opts.check)?;
        if result {
            needs_format = true;
        }
    }

    if opts.check && needs_format {
        std::process::exit(1);
    }

    Ok(())
}

fn find_lua_files(dir: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_lua_files(std::path::Path::new(dir), &mut files)?;
    Ok(files)
}

fn collect_lua_files(dir: &std::path::Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                collect_lua_files(&path, files)?;
            } else if path.extension().is_some_and(|ext| ext == "lua") {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn format_file(file: &PathBuf, check: bool) -> Result<bool> {
    let code = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    let formatted = format_code(&code);

    if check {
        if code != formatted {
            println!("Would reformat: {}", file.display());
            Ok(true)
        } else {
            Ok(false)
        }
    } else if code != formatted {
        std::fs::write(file, &formatted)
            .with_context(|| format!("Failed to write file: {}", file.display()))?;
        println!("Formatted: {}", file.display());
        Ok(true)
    } else {
        Ok(false)
    }
}
