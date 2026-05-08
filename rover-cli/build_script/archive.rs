use crate::build_script::embedded;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs;
use std::io::Write;
use std::path::Path;
use tar::{Builder, Header};

pub fn write_runtime_archive(
    out_path: &Path,
    wasm_js: &Path,
    wasm_bin: &Path,
) -> std::io::Result<()> {
    let file = fs::File::create(out_path)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);

    append_str(&mut tar, "index.html", embedded::runtime_index_html())?;
    append_str(&mut tar, "loader.js", embedded::runtime_loader_js())?;
    tar.append_path_with_name(wasm_js, "rover_web_wasm.js")?;
    tar.append_path_with_name(wasm_bin, "rover_web_wasm.wasm")?;

    finish_archive(tar)
}

pub fn write_placeholder_archive(out_path: &Path) -> std::io::Result<()> {
    let file = fs::File::create(out_path)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);

    append_str(&mut tar, "index.html", embedded::runtime_index_html())?;
    append_str(&mut tar, "loader.js", embedded::runtime_loader_js())?;
    append_str(&mut tar, "rover_web_wasm.js", "")?;
    append_str(&mut tar, "rover_web_wasm.wasm", "")?;

    finish_archive(tar)
}

fn finish_archive(tar: Builder<GzEncoder<fs::File>>) -> std::io::Result<()> {
    let encoder = tar.into_inner()?;
    let mut file = encoder.finish()?;
    file.flush()?;
    Ok(())
}

fn append_str<W: Write>(tar: &mut Builder<W>, path: &str, content: &str) -> std::io::Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, path, content.as_bytes())?;
    Ok(())
}
