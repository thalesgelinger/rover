fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR missing");
    let target_path = std::path::PathBuf::from(out_dir)
        .parent()
        .expect("missing parent 1")
        .parent()
        .expect("missing parent 2")
        .parent()
        .expect("missing parent 3")
        .join("rover_web_wasm");

    println!("cargo:rustc-link-arg=-sEXPORTED_RUNTIME_METHODS=['cwrap','ccall']");
    println!("cargo:rustc-link-arg=-sEXPORTED_FUNCTIONS=['_rover_web_init','_rover_web_load_lua','_rover_web_tick']");
    println!("cargo:rustc-link-arg=-sEXPORT_ES6=1");
    println!("cargo:rustc-link-arg=--no-entry");
    println!(
        "cargo:rustc-link-arg=-o{}.js",
        target_path.to_string_lossy()
    );
}
