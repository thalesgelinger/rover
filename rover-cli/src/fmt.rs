use anyhow::Result;
use rover_parser::format_code;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn run_fmt(options: FmtOptions) -> Result<()> {
    let input = read_file(&options.file)?;
    let formatted = format_code(&input);
    
    if options.check {
        if input == formatted {
            println!("File is already formatted");
        } else {
            eprintln!("File is not formatted");
            return Err(anyhow::anyhow!("Formatting differences found"));
        }
    } else {
        fs::write(&options.file, formatted)?;
        println!("Formatted: {}", options.file.display());
    }
    
    Ok(())
}

fn read_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

pub struct FmtOptions {
    pub file: PathBuf,
    pub check: bool,
}
